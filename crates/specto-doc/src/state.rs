//! What was done to a document, without the history of how: what a draft keeps of it.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specto_annot::StampImage;

use crate::annots::AnnotEdits;
use crate::{EditDoc, FieldId, FieldValue, PageRef, SourceId};

/// The pages, the annotation edits and what was filled into the form of a document as edited,
/// without undo and redo.
///
/// It can be stored and read back (see `specto-store`), and makes an [`EditDoc`] again with
/// [`EditDoc::restored`]. Sources are named by number, as in the document it came from: whoever
/// stores it keeps their files to go with it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditState {
    pages: Vec<PageRef>,
    annots: AnnotEdits,
    #[serde(with = "pairs")]
    form: BTreeMap<FieldId, FieldValue>,
    next_annot: u64,
}

impl EditState {
    pub fn pages(&self) -> &[PageRef] {
        &self.pages
    }

    /// The sources that pages come from, in order of first use.
    pub fn sources(&self) -> Vec<SourceId> {
        let mut out: Vec<SourceId> = Vec::new();
        for p in &self.pages {
            if !out.contains(&p.source) {
                out.push(p.source);
            }
        }
        out
    }

    /// Gives each source the number `new` has for it.
    ///
    /// What was done on pages of sources it has none for is dropped. Every source of a page must
    /// have one.
    pub fn renumber_sources(&mut self, new: impl Fn(SourceId) -> Option<SourceId>) {
        for p in &mut self.pages {
            p.source = new(p.source).expect("every source of a page has a new number");
        }
        self.annots.renumber_sources(new);
    }

    /// The images of the stamps among the annotations, each once: what has to be kept with the
    /// state, which only names them.
    pub fn stamp_images(&self) -> Vec<Arc<StampImage>> {
        self.annots.stamp_images()
    }
}

impl EditDoc {
    /// What was done to the document, without undo and redo.
    pub fn state(&self) -> EditState {
        EditState {
            pages: self.pages.clone(),
            annots: (*self.annots).clone(),
            form: (*self.form).clone(),
            next_annot: self.next_annot,
        }
    }

    /// A document as `state` has it, with nothing to undo, and changed from what is on disk.
    pub fn restored(state: EditState) -> Self {
        Self {
            pages: state.pages,
            annots: Arc::new(state.annots),
            form: Arc::new(state.form),
            next_annot: state.next_annot,
            version: 0,
            next_version: 1,
            saved: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

/// A map written as a list of pairs, for keys that aren't text, which JSON requires of a map.
pub(crate) mod pairs {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize,
        V: Serialize,
        S: Serializer,
    {
        serializer.collect_seq(map)
    }

    pub fn deserialize<'de, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(K, V)>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use specto_annot::{AnnotKind, Annotation, Origin};
    use specto_geom::{PdfBox, PdfPoint, Rotation};

    use super::*;
    use crate::AnnotId;

    const MAIN: SourceId = SourceId(0);
    const OTHER: SourceId = SourceId(1);
    const GONE: SourceId = SourceId(2);

    fn note() -> Annotation {
        Annotation::new(
            AnnotKind::Note,
            PdfBox::new(PdfPoint::new(10.0, 10.0), PdfPoint::new(28.0, 28.0)),
        )
    }

    /// A document with its pages turned, one inserted, an annotation of the file changed, one
    /// added, and a field filled in.
    fn edited() -> EditDoc {
        let mut doc = EditDoc::from_source(MAIN, 3);
        doc.rotate(&[1], Rotation::Cw90);
        doc.insert(3, vec![PageRef::new(OTHER, 0)]);
        let mut changed = note();
        changed.origin = Some(Origin {
            index: 0,
            object: Some((12, 0)),
        });
        let id = AnnotId::File {
            source: MAIN,
            page: 0,
            index: 0,
        };
        doc.change_annotation(id, changed);
        doc.add_annotation(OTHER, 0, note());
        doc.fill_field(FieldId(7, 0), FieldValue::Text("Ada".into()), None);
        doc
    }

    #[test]
    fn a_document_is_restored_with_its_edits() {
        let doc = edited();
        let json = serde_json::to_string(&doc.state()).unwrap();
        let state: EditState = serde_json::from_str(&json).unwrap();
        let restored = EditDoc::restored(state);

        assert_eq!(restored.pages(), doc.pages());
        assert_eq!(restored.form_values(), doc.form_values());
        for (source, page) in [(MAIN, 0), (OTHER, 0)] {
            assert_eq!(
                restored.annotation_changes(source, page),
                doc.annotation_changes(source, page)
            );
        }
        assert!(restored.is_modified());
        assert_eq!(restored.undo_edit(), None);
        // New annotations don't take the number of one there is.
        let mut restored = restored;
        assert_eq!(
            restored.add_annotation(MAIN, 1, note()),
            AnnotId::New(doc.next_annot)
        );
    }

    #[test]
    fn sources_are_numbered_anew() {
        let mut doc = edited();
        doc.add_annotation(GONE, 0, note());
        let mut state = doc.state();
        state.renumber_sources(|s| match s {
            MAIN => Some(MAIN),
            OTHER => Some(SourceId(5)),
            _ => None,
        });
        let restored = EditDoc::restored(state);
        assert_eq!(restored.pages()[3].source, SourceId(5));
        assert_eq!(restored.annotation_changes(SourceId(5), 0).added.len(), 1);
        assert_eq!(
            restored.pages_with_annotation_edits(),
            vec![(MAIN, 0), (SourceId(5), 0)]
        );
    }
}
