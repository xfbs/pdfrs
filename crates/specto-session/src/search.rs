//! Searching the document.
//!
//! Matching itself is in `specto_text` ([`SearchText`]) and `specto_view` ([`SearchResults`], the
//! list of matches and the current one). Here is what goes around it: every page's search text,
//! prepared by a background thread and kept per file, and the search in progress.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use specto_doc::SourceId;
use specto_source::PdfData;
use specto_text::{Extractor, Query, SearchText};
use specto_view::{SearchMatch, SearchResults, TextPages};

use crate::{FilePage, Session};

/// A search: the query and what it found so far.
pub(crate) struct Search {
    query: Query,
    results: SearchResults,
    /// The page the search started from: the first match after it is the first one shown.
    start: usize,
    /// The view went to a match, and where it was before is remembered for going back.
    left: bool,
}

/// Flags shared with the search indexer thread.
#[derive(Default)]
pub(crate) struct Indexer {
    /// Set to stop it.
    stop: AtomicBool,
    /// Set by the thread when it's done.
    done: AtomicBool,
}

/// What the search indexer hands over: a page of one of the files and its search text. Pass it to
/// [`Session::indexed`].
pub struct Indexed {
    page: FilePage,
    text: SearchText,
}

/// How a search stands, for a search bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchStatus {
    pub matches: usize,
    /// Index of the current match.
    pub current: Option<usize>,
    pub pages_indexed: usize,
    pub pages: usize,
}

impl Session {
    /// The search text of page `page` of a file, if indexed.
    fn search_text(&self, (source, page): FilePage) -> Option<&SearchText> {
        self.sources
            .get(source.0 as usize)?
            .index
            .get(page as usize)?
            .as_ref()
    }

    /// Starts indexing the document's pages not indexed yet in a background thread, from position
    /// `from` onwards and then from the start, unless that is already running or done. Returns
    /// whether it started.
    ///
    /// The thread parses the files for itself and shares nothing with the session. It calls `sink`
    /// with each page's search text: get that to whoever holds the session and pass it to
    /// [`indexed`](Self::indexed). The thread ends when `sink` returns `false`, when the session is
    /// dropped, or when the page list changes in a way that needs another start (a file was added).
    pub fn start_indexing(
        &mut self,
        from: usize,
        mut sink: impl FnMut(Indexed) -> bool + Send + 'static,
    ) -> bool {
        if self
            .indexer
            .as_ref()
            .is_some_and(|i| !i.done.load(Ordering::Relaxed))
        {
            return false;
        }
        let refs = self.edit.pages();
        let from = from.min(refs.len());
        let mut seen = HashSet::new();
        let pages: Vec<FilePage> = refs[from..]
            .iter()
            .chain(&refs[..from])
            .map(|p| (p.source, p.page))
            .filter(|&p| self.search_text(p).is_none() && seen.insert(p))
            .collect();
        if pages.is_empty() {
            return false;
        }
        // Each file the thread needs, to open its own copy of.
        let files: HashMap<SourceId, (PdfData, Option<String>)> = pages
            .iter()
            .map(|&(id, _)| {
                let s = &self.sources[id.0 as usize];
                (id, (s.data.clone(), s.password.clone()))
            })
            .collect();
        let indexer = Arc::new(Indexer::default());
        let flags = indexer.clone();
        let spawned = std::thread::Builder::new()
            .name("search index".into())
            .spawn(move || {
                struct Done(Arc<Indexer>);
                impl Drop for Done {
                    fn drop(&mut self) {
                        self.0.done.store(true, Ordering::Relaxed);
                    }
                }
                let done = Done(flags);
                let stop = &done.0.stop;
                let mut pdfs = HashMap::new();
                for (id, (data, password)) in files {
                    if let Ok(pdf) = specto_source::open_pdf(data, password.as_deref()) {
                        pdfs.insert(id, pdf);
                    }
                }
                for page in pages {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let Some(pdf) = pdfs.get(&page.0) else {
                        continue;
                    };
                    let text = Extractor::new(pdf)
                        .page(page.1 as usize)
                        .map(|t| SearchText::new(&t))
                        .unwrap_or_default();
                    if !sink(Indexed { page, text }) {
                        return;
                    }
                }
            });
        if spawned.is_err() {
            return false;
        }
        self.indexer = Some(indexer);
        true
    }

    /// Stops the indexer thread, if one is running.
    pub(crate) fn stop_indexing(&mut self) {
        if let Some(i) = self.indexer.take() {
            i.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Takes in a page from the indexer.
    ///
    /// Returns whether a search had no current match before and has one now (so the view should go
    /// there).
    pub fn indexed(&mut self, Indexed { page, text }: Indexed) -> bool {
        let Some(slot) = self
            .sources
            .get_mut(page.0.0 as usize)
            .and_then(|s| s.index.get_mut(page.1 as usize))
        else {
            return false;
        };
        // Two indexers can hand over the same page: one stopped with pages still on their way,
        // and the one started after it. The page's matches are counted once.
        if slot.is_some() {
            return false;
        }
        *slot = Some(text);
        let text = slot.as_ref().expect("just set");
        let Some(s) = self.search.as_mut() else {
            return false;
        };
        let had = s.results.current().is_some();
        let found = text.find(&s.query);
        for (i, p) in self.edit.pages().iter().enumerate() {
            if (p.source, p.page) == page {
                s.results.add_page(i, found.clone());
            }
        }
        if !had {
            s.results.select_from(s.start);
        }
        !had && s.results.current().is_some()
    }

    /// How many of the document's pages are indexed.
    fn pages_indexed(&self) -> usize {
        self.edit
            .pages()
            .iter()
            .filter(|p| self.search_text((p.source, p.page)).is_some())
            .count()
    }

    /// Searches for `text` in the pages indexed so far (the rest follow as they're indexed).
    ///
    /// An empty `text` ends the search. Returns whether there is a current match to go to.
    pub fn search(&mut self, text: &str) -> bool {
        let Some(query) = Query::new(text) else {
            self.search = None;
            return false;
        };
        let start = self.viewer.current_page();
        let results = self.find_all(&query, start);
        let found = results.current().is_some();
        self.search = Some(Search {
            query,
            results,
            start,
            left: self.search.as_ref().is_some_and(|s| s.left),
        });
        found
    }

    /// The matches of `query` on the pages indexed so far, from page `start` on.
    fn find_all(&self, query: &Query, start: usize) -> SearchResults {
        let mut results = SearchResults::new();
        for (i, p) in self.edit.pages().iter().enumerate() {
            if let Some(t) = self.search_text((p.source, p.page)) {
                results.add_page(i, t.find(query));
            }
        }
        results.select_from(start);
        results
    }

    pub fn end_search(&mut self) {
        self.search = None;
    }

    /// Runs the current search again, after the page list changed: its results hold positions.
    pub(crate) fn search_again(&mut self) {
        if let Some(s) = self.search.take() {
            let start = self.viewer.current_page();
            let results = self.find_all(&s.query, start);
            self.search = Some(Search {
                query: s.query,
                results,
                start,
                left: s.left,
            });
        }
    }

    /// Moves to the next (or previous) match. Returns whether there is one.
    pub fn search_step(&mut self, forward: bool) -> bool {
        let Some(s) = self.search.as_mut() else {
            return false;
        };
        let m = if forward {
            s.results.select_next()
        } else {
            s.results.select_previous()
        };
        m.is_some()
    }

    /// Scrolls the current match into view.
    pub fn reveal_match(&mut self) {
        let Some(m) = self.current_match() else {
            return;
        };
        let area = {
            let (_, mut texts, _) = self.text();
            let Some(text) = texts.text_page(m.page) else {
                return;
            };
            let quads = text.selection_quads(m.chars.clone());
            specto_geom::PdfBox::from_points(quads.iter().flat_map(|q| q.points))
        };
        // Back returns to where the search began, not to each match on the way.
        if self.search.as_ref().is_some_and(|s| s.left) {
            self.viewer.reveal(m.page, area);
        } else if self.viewer.jump(|v| v.reveal(m.page, area))
            && let Some(s) = self.search.as_mut()
        {
            s.left = true;
        }
    }

    pub fn current_match(&self) -> Option<SearchMatch> {
        let (_, m) = self.search.as_ref()?.results.current()?;
        Some(m.clone())
    }

    /// The matches on `page`, with whether each is the current one.
    pub fn matches_on(&self, page: usize) -> Vec<(SearchMatch, bool)> {
        self.search.as_ref().map_or_else(Vec::new, |s| {
            s.results
                .on_page(page)
                .map(|(m, current)| (m.clone(), current))
                .collect()
        })
    }

    /// How the search stands, if there is one.
    pub fn search_status(&self) -> Option<SearchStatus> {
        let s = self.search.as_ref()?;
        Some(SearchStatus {
            matches: s.results.len(),
            current: s.results.current().map(|(i, _)| i),
            pages_indexed: self.pages_indexed(),
            pages: self.edit.page_count(),
        })
    }
}
