//! Editing pages: rotating, deleting, moving, inserting and exporting them.

use super::*;

/// Where the page at `page` is after moving those at `moved` so that they start at `start`.
pub(super) fn position_after_move(moved: &[usize], start: usize, page: usize) -> usize {
    let mut moved = moved.to_vec();
    moved.sort_unstable();
    moved.dedup();
    if let Some(i) = moved.iter().position(|&m| m == page) {
        return start + i;
    }
    let rest = page - moved.iter().filter(|&&m| m < page).count();
    if rest >= start {
        rest + moved.len()
    } else {
        rest
    }
}

/// The Undo menu item for an edit, such as "Undo Rotate Page" or "Undo Delete 3 Pages".
///
/// Whole sentences per edit, so each can be translated as one.
pub(super) fn undo_label(edit: Edit) -> String {
    match edit {
        Edit::Rotate { pages } => {
            ngettext_f("Undo Rotate Page", "Undo Rotate {n} Pages", pages, &[])
        }
        Edit::Delete { pages } => {
            ngettext_f("Undo Delete Page", "Undo Delete {n} Pages", pages, &[])
        }
        Edit::Move { pages } => ngettext_f("Undo Move Page", "Undo Move {n} Pages", pages, &[]),
        Edit::Insert { pages } => {
            ngettext_f("Undo Insert Page", "Undo Insert {n} Pages", pages, &[])
        }
        Edit::AddAnnotation(AnnotType::Highlight) => gettext("Undo Highlight"),
        Edit::AddAnnotation(AnnotType::Underline) => gettext("Undo Underline"),
        Edit::AddAnnotation(AnnotType::StrikeOut) => gettext("Undo Strikethrough"),
        Edit::AddAnnotation(AnnotType::Ink) => gettext("Undo Drawing"),
        Edit::AddAnnotation(AnnotType::Square) => gettext("Undo Rectangle"),
        Edit::AddAnnotation(AnnotType::Circle) => gettext("Undo Ellipse"),
        Edit::AddAnnotation(AnnotType::Line) => gettext("Undo Line"),
        Edit::AddAnnotation(AnnotType::Note) => gettext("Undo Note"),
        Edit::AddAnnotation(AnnotType::FreeText) => gettext("Undo Text Box"),
        Edit::AddAnnotation(_) => gettext("Undo Add Annotation"),
        Edit::RemoveAnnotation(_) => gettext("Undo Delete Annotation"),
        Edit::ChangeAnnotation(_) => gettext("Undo Change Annotation"),
        Edit::FillField => gettext("Undo Fill In Field"),
        Edit::ResetForm => gettext("Undo Reset Form"),
    }
}

/// The Redo menu item for an edit, like [`undo_label`].
pub(super) fn redo_label(edit: Edit) -> String {
    match edit {
        Edit::Rotate { pages } => {
            ngettext_f("Redo Rotate Page", "Redo Rotate {n} Pages", pages, &[])
        }
        Edit::Delete { pages } => {
            ngettext_f("Redo Delete Page", "Redo Delete {n} Pages", pages, &[])
        }
        Edit::Move { pages } => ngettext_f("Redo Move Page", "Redo Move {n} Pages", pages, &[]),
        Edit::Insert { pages } => {
            ngettext_f("Redo Insert Page", "Redo Insert {n} Pages", pages, &[])
        }
        Edit::AddAnnotation(AnnotType::Highlight) => gettext("Redo Highlight"),
        Edit::AddAnnotation(AnnotType::Underline) => gettext("Redo Underline"),
        Edit::AddAnnotation(AnnotType::StrikeOut) => gettext("Redo Strikethrough"),
        Edit::AddAnnotation(AnnotType::Ink) => gettext("Redo Drawing"),
        Edit::AddAnnotation(AnnotType::Square) => gettext("Redo Rectangle"),
        Edit::AddAnnotation(AnnotType::Circle) => gettext("Redo Ellipse"),
        Edit::AddAnnotation(AnnotType::Line) => gettext("Redo Line"),
        Edit::AddAnnotation(AnnotType::Note) => gettext("Redo Note"),
        Edit::AddAnnotation(AnnotType::FreeText) => gettext("Redo Text Box"),
        Edit::AddAnnotation(_) => gettext("Redo Add Annotation"),
        Edit::RemoveAnnotation(_) => gettext("Redo Delete Annotation"),
        Edit::ChangeAnnotation(_) => gettext("Redo Change Annotation"),
        Edit::FillField => gettext("Redo Fill In Field"),
        Edit::ResetForm => gettext("Redo Reset Form"),
    }
}

/// An edit's name in the tests' `debug-state` (such as "Rotate Page" or "Delete 3 Pages"): always
/// English, and not shown to anyone.
pub(super) fn edit_name(edit: Edit) -> String {
    let (verb, n) = match edit {
        Edit::Rotate { pages } => ("Rotate", pages),
        Edit::Delete { pages } => ("Delete", pages),
        Edit::Move { pages } => ("Move", pages),
        Edit::Insert { pages } => ("Insert", pages),
        Edit::AddAnnotation(t) => return format!("Add {t:?}"),
        Edit::RemoveAnnotation(t) => return format!("Remove {t:?}"),
        Edit::ChangeAnnotation(t) => return format!("Change {t:?}"),
        Edit::FillField => return "FillField".to_owned(),
        Edit::ResetForm => return "ResetForm".to_owned(),
    };
    if n == 1 {
        format!("{verb} Page")
    } else {
        format!("{verb} {n} Pages")
    }
}

/// The name Export Selected Pages suggests: "Report (pages 2-4, 7).pdf" for positions 1, 2, 3 and 6
/// of "Report.pdf".
pub(super) fn export_name(name: &str, pages: &[usize]) -> String {
    let stem = name
        .strip_suffix(".pdf")
        .or_else(|| name.strip_suffix(".PDF"))
        .unwrap_or(name);
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &p in pages {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == p => *end = p,
            _ => ranges.push((p, p)),
        }
    }
    let what = match ranges.as_slice() {
        [(a, b)] if a == b => gettext_f("page {page}", &[("page", &(a + 1))]),
        r if r.len() <= 3 => {
            let parts: Vec<String> = r
                .iter()
                .map(|&(a, b)| {
                    if a == b {
                        (a + 1).to_string()
                    } else {
                        format!("{}-{}", a + 1, b + 1)
                    }
                })
                .collect();
            gettext_f("pages {pages}", &[("pages", &parts.join(", "))])
        }
        _ => ngettext_f("{n} page", "{n} pages", pages.len(), &[]),
    };
    format!("{stem} ({what}).pdf")
}

impl Window {
    /// The pages an edit applies to: those selected in the sidebar if there are several, else the
    /// current page (both pages of a spread).
    pub(super) fn pages_to_edit(&self) -> Vec<usize> {
        let selected = self
            .0
            .sidebar
            .borrow()
            .as_ref()
            .map(Sidebar::selected)
            .unwrap_or_default();
        if selected.len() > 1 {
            return selected;
        }
        self.0
            .canvas
            .with_viewer(|v| v.current_spread().to_vec())
            .unwrap_or_default()
    }

    /// Delete Pages of the menu, and Delete and Backspace where they delete: the annotation that is
    /// picked, else pages. In the list of markup, the annotation of the marked row, and never
    /// pages. On the thumbnails, pages whatever is picked.
    pub(super) fn delete_pages(&self) {
        let inner = &self.0;
        let on_thumbnails = (inner.sidebar.borrow().as_ref())
            .zip(gtk::prelude::GtkWindowExt::focus(&inner.window))
            .is_some_and(|(s, f)| f.is_ancestor(&s.widget) || f == s.widget);
        let on_markup = inner
            .sidebar_stack
            .borrow()
            .as_ref()
            .and_then(|s| s.visible_child_name())
            .is_some_and(|name| name == "markup");
        let on_page = gtk::prelude::GtkWindowExt::focus(&inner.window)
            .is_some_and(|f| f.is_ancestor(&inner.canvas) || f == inner.canvas);
        let in_list = on_markup && inner.split.shows_sidebar() && !on_page;
        let row = inner
            .markup_list
            .borrow()
            .as_ref()
            .filter(|_| in_list)
            .and_then(MarkupList::selected);
        if let Some((_, (position, id))) = row {
            inner.canvas.pick_and_show(position, id);
        }
        let mut failed = None;
        let mut removed = false;
        // As after an edit to pages: the thumbnail of the page changes too.
        self.edit(|d| {
            if on_thumbnails {
                return None;
            }
            match d.remove_picked_annotation() {
                Ok(r) => removed = r,
                Err(e) => failed = Some(e),
            }
            removed.then(Edited::default)
        });
        if let Some(e) = failed {
            return self.toast(&gettext_f(
                "Couldn't delete the annotation: {error}",
                &[("error", &save_error(e))],
            ));
        }
        if in_list {
            // The next row takes the place of the deleted one.
            match (removed, row, inner.markup_list.borrow().as_ref()) {
                (true, Some((index, _)), Some(list)) => list.focus_row(index),
                (true, ..) => {}
                (false, ..) => inner.window.error_bell(),
            }
            return;
        }
        if removed {
            return;
        }
        let selected = self
            .0
            .sidebar
            .borrow()
            .as_ref()
            .map(Sidebar::selected)
            .unwrap_or_default();
        let pages = if selected.len() > 1 {
            selected
        } else {
            match self.0.canvas.with_viewer(|v| v.current_page()) {
                Some(p) => vec![p],
                None => return,
            }
        };
        let mut deleted = false;
        self.edit(|d| {
            deleted = d.delete(&pages);
            deleted.then(Edited::default)
        });
        if !deleted {
            self.0.window.error_bell();
        }
    }

    /// Inserts pages of another document before position `to`.
    ///
    /// They're selected, and the view goes to the first of them.
    pub(super) fn insert_bundle(&self, bundle: &PageBundle, to: usize) {
        let mut error = None;
        self.edit(|d| match d.insert_bundle(to, bundle) {
            Ok(range) => {
                let range = range?;
                Some(Edited {
                    go_to: Some(range.start),
                    select: range.collect(),
                })
            }
            Err(e) => {
                error = Some(e);
                None
            }
        });
        match error {
            Some(e) => self.toast(&gettext_f(
                "Couldn't copy the pages: {error}",
                &[("error", &e)],
            )),
            None => self.index_for_search(),
        }
    }

    /// Moves the pages at `pages` before position `to`.
    ///
    /// They stay selected, and the view stays on the page it showed.
    pub(super) fn move_pages(&self, pages: &[usize], to: usize) {
        let Some(current) = self.0.canvas.with_viewer(|v| v.current_page()) else {
            return;
        };
        self.edit(|d| {
            let moved = d.move_pages(pages, to)?;
            Some(Edited {
                go_to: Some(position_after_move(pages, moved.start, current)),
                select: moved.collect(),
            })
        });
    }

    /// Whether the sidebar has the keyboard focus.
    pub(super) fn sidebar_has_focus(&self) -> bool {
        let focus = gtk::prelude::GtkWindowExt::focus(&self.0.window);
        let sidebar = self.0.sidebar.borrow();
        matches!((focus, sidebar.as_ref()), (Some(f), Some(s)) if s.contains(&f))
    }

    /// Turns the pages to edit (see [`pages_to_edit`]) by `by`, unless a text field has the focus:
    /// there the shortcut (Ctrl+Left/Right) moves by a word, as it would without this action.
    ///
    /// [`pages_to_edit`]: Self::pages_to_edit
    pub(super) fn rotate(&self, by: Rotation) {
        // In a text field, the keys move the cursor by words instead.
        let focus = gtk::prelude::GtkWindowExt::focus(&self.0.window);
        if let Some(text) = focus.filter(|f| f.is::<gtk::Text>() || f.is::<gtk::TextView>()) {
            let count = if by == Rotation::Cw90 { 1 } else { -1 };
            text.emit_by_name::<()>("move-cursor", &[&gtk::MovementStep::Words, &count, &false]);
            return;
        }
        let pages = self.pages_to_edit();
        // The pages stay selected, so they can be turned again.
        self.edit(|d| {
            d.rotate(&pages, by).then(|| Edited {
                select: pages.clone(),
                go_to: None,
            })
        });
    }

    /// Shows the pages with their margins trimmed, or whole.
    pub(super) fn set_trim_margins(&self, on: bool) {
        let inner = &self.0;
        if let Some(a) = inner
            .window
            .lookup_action("trim-margins")
            .and_downcast::<gio::SimpleAction>()
        {
            a.set_state(&on.to_variant());
        }
        let Some(doc) = inner.document.borrow().clone() else {
            return;
        };
        if doc.borrow_mut().set_trim_margins(on) {
            inner.canvas.update_viewer(|_| {});
        }
    }

    /// Asks for a PDF and inserts its pages.
    pub(super) async fn choose_insert_file(&self) {
        let dialog = pdf_dialog(&gettext("Insert Pages from File"));
        if let Some(folder) = self
            .0
            .uri
            .borrow()
            .as_ref()
            .and_then(|u| folder(&gio::File::for_uri(u)))
        {
            dialog.set_initial_folder(Some(&folder));
        }
        if let Ok(file) = dialog.open_future(Some(&self.0.window)).await {
            self.insert_file(file).await;
        }
    }

    /// Inserts all pages of `file` after the current page (or spread) and goes to the first of
    /// them.
    pub(super) async fn insert_file(&self, file: gio::File) {
        self.insert_files(vec![file], None).await;
    }

    /// Inserts all pages of `files`, in order and as one edit, before position `at` (by default
    /// after the current page or spread), and goes to the first of them.
    ///
    /// A file that can't be opened is left out, with a message.
    pub(super) async fn insert_files(&self, files: Vec<gio::File>, at: Option<usize>) {
        let Some(doc) = self.0.document.borrow().clone() else {
            return;
        };
        let mut sources = Vec::new();
        for file in files {
            if let Some(source) = self.open_source(&doc, &file).await {
                sources.push(source);
            }
        }
        // The window may have moved on to another document meanwhile.
        if sources.is_empty()
            || !self
                .0
                .document
                .borrow()
                .as_ref()
                .is_some_and(|d| Rc::ptr_eq(d, &doc))
        {
            return;
        }
        let at = at.unwrap_or_else(|| {
            self.0
                .canvas
                .with_viewer(|v| v.current_spread().last().map_or(0, |&p| p + 1))
                .unwrap_or(0)
        });
        // The new pages are selected, ready to be moved or turned.
        self.edit(|d| {
            let inserted = d.insert(at, &sources)?;
            Some(Edited {
                go_to: Some(inserted.start),
                select: inserted.collect(),
            })
        });
        self.index_for_search();
    }

    /// Reads and opens `file` as a source of `doc`, asking for its password if it has one.
    ///
    /// `None` if it couldn't be opened (with a message) or the password was canceled.
    pub(super) async fn open_source(
        &self,
        doc: &SharedDocument,
        file: &gio::File,
    ) -> Option<SourceId> {
        let name = display_name(file).await;
        let data = match read_pdf(file).await {
            Ok(data) => data,
            Err(e) => {
                self.toast(&couldnt_read(&name, &e));
                return None;
            }
        };
        let mut password: Option<String> = None;
        loop {
            let opened = doc
                .borrow_mut()
                .add_source(data.clone(), password.as_deref());
            match opened {
                Ok((source, _)) => return Some(source),
                Err(OpenError::PasswordRequired) => {
                    password = Some(self.ask_password(&name, password.is_some()).await?);
                }
                Err(e) => {
                    self.toast(&gettext_f(
                        "Couldn't open “{name}”: {error}",
                        &[("name", &name), ("error", &e)],
                    ));
                    return None;
                }
            }
        }
    }

    /// The pages "Export Selected Pages" writes: those selected in the sidebar, else the current
    /// page.
    pub(super) fn pages_to_export(&self) -> Vec<usize> {
        let selected = self
            .0
            .sidebar
            .borrow()
            .as_ref()
            .map(Sidebar::selected)
            .unwrap_or_default();
        if !selected.is_empty() {
            return selected;
        }
        self.0
            .canvas
            .with_viewer(|v| vec![v.current_page()])
            .unwrap_or_default()
    }

    /// Asks where to save the selected pages, and saves them there as a new PDF.
    pub(super) async fn export_pages(&self) {
        self.finish_text_entry();
        let inner = &self.0;
        let Some(name) = inner
            .document
            .borrow()
            .as_ref()
            .map(|d| d.borrow().name.clone())
        else {
            return;
        };
        let pages = self.pages_to_export();
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Export Selected Pages"))
            .initial_name(export_name(&name, &pages))
            .build();
        if let Some(folder) = inner
            .uri
            .borrow()
            .as_ref()
            .and_then(|u| folder(&gio::File::for_uri(u)))
        {
            dialog.set_initial_folder(Some(&folder));
        }
        if let Ok(file) = dialog.save_future(Some(&inner.window)).await {
            self.export_to(&pages, file).await;
        }
    }

    /// Saves the pages at `pages` as a new PDF in `file`. The document and its file stay as they
    /// are.
    pub(super) async fn export_to(&self, pages: &[usize], file: gio::File) {
        let Some(doc) = self.0.document.borrow().clone() else {
            return;
        };
        let job = {
            let d = doc.borrow();
            let refs = pages
                .iter()
                .filter_map(|&i| d.edit().pages().get(i).copied())
                .collect();
            d.save_pages(refs)
        };
        let n = pages.len();
        match write_job(job, &file).await {
            Ok(()) => self.toast(&ngettext_f(
                "Exported {n} page to “{name}”",
                "Exported {n} pages to “{name}”",
                n,
                &[("name", &file_name(&file))],
            )),
            Err(e) => self.toast(&couldnt_save(&file, &e)),
        }
    }
}
