//! Saving by appending: the file as it is, followed by the objects that changed.
//!
//! Much faster than a rewrite for a large file, and what the file held before is untouched, so a
//! digital signature over it stays valid. It is also why this is only done where a save adds to
//! the document and takes nothing away: new annotations, and values for form fields that had
//! none. Anything deleted or changed would stay in the file for someone to dig out, so those saves
//! rewrite it.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use lopdf::{Dictionary, Document, IncrementalDocument, LoadOptions, Object, ObjectId};
use specto_doc::{FieldId, FieldValue, PageRef, Rotation};

use crate::{AnnotChanges, Source, WriteError, annotate, forms, verify};

/// The file of `main` with what `annots` adds and `form` fills in appended, or `None` where the
/// save has to be a rewrite: pages were moved, turned, deleted or inserted, something that was in
/// the file is deleted or changed, there is nothing to add, or the file is one lopdf had to repair
/// to read.
///
/// The result is checked like a rewrite is: read back, with every page that was touched compared
/// to the page it was.
pub fn append(
    main: &Source,
    pages: &[PageRef],
    annots: &AnnotChanges,
    form: &[(FieldId, FieldValue)],
) -> Result<Option<Vec<u8>>, WriteError> {
    let in_place = (pages.iter().enumerate())
        .all(|(i, p)| p.source == main.id && p.page as usize == i && p.rotation == Rotation::None);
    let changes = || annots.iter().filter(|(_, c)| !c.is_empty());
    let only_adds = changes().all(|((source, _), c)| {
        *source == main.id && c.removed.is_empty() && c.replaced.is_empty()
    });
    if !in_place || !only_adds || (changes().next().is_none() && form.is_empty()) {
        return Ok(None);
    }

    // As the file is: what isn't touched is never written, so nothing of it needs repairing.
    let options = LoadOptions {
        password: main.password.map(str::to_owned),
        ..LoadOptions::default()
    };
    let Ok(mut doc) = Document::load_mem_with_options(main.data, options) else {
        return Ok(None);
    };
    if doc.is_encrypted()
        || doc.xref_start == 0
        || doc.xref_recovery.is_some()
        || doc.get_pages().len() != pages.len()
        || form.iter().any(|(field, _)| forms::has_value(&doc, *field))
    {
        return Ok(None);
    }
    let encrypted = doc.encryption_state.is_some();

    // What the file's objects are before the changes, to tell which of them change. Of a stream
    // only its dictionary and length: its data may be most of a large file, and nothing here
    // rewrites the data of a stream the file has.
    let before: HashMap<ObjectId, Before> = (doc.objects.iter())
        .map(|(id, object)| (*id, Before::of(object)))
        .collect();
    forms::apply(&mut doc, form)?;
    annotate(&mut doc, main.id, annots)?;
    let mut changed = Vec::new();
    for (id, object) in &doc.objects {
        match before.get(id) {
            Some(was) if was.is(object) => {}
            // A stream of the file: lopdf may have read its data wrongly, and it would be
            // written as read.
            Some(Before::Stream(..)) => return Ok(None),
            _ => changed.push((*id, object.clone())),
        }
    }
    if changed.is_empty() {
        return Ok(None);
    }

    // lopdf writes the file and then what changed. It only passes the file's bytes through, so it
    // gets a stand-in of the same length and ending, and what it writes of it is left out: a
    // large file isn't copied twice. From here on `doc` only says how the file is laid out.
    let file: &[u8] = main.data;
    let mut stand_in = vec![0; file.len()];
    if let (Some(last), Some(byte)) = (stand_in.last_mut(), file.last()) {
        *last = *byte;
    }
    let max_id = doc.max_id;
    let mut update = IncrementalDocument::create_from(stand_in, doc);
    update.new_document.max_id = max_id;
    update.new_document.objects.extend(changed);
    let mut tail = Tail {
        skip: file.len(),
        bytes: Vec::new(),
    };
    update.save_to(&mut tail)?;
    let mut out = Vec::with_capacity(file.len() + tail.bytes.len());
    out.extend_from_slice(file);
    out.extend_from_slice(&tail.bytes);

    // A form's widgets can be on any page.
    let touched: Option<HashSet<usize>> = form.is_empty().then(|| {
        changes()
            .map(|((_, page), _)| *page as usize)
            .collect::<HashSet<_>>()
    });
    verify(&out, encrypted, &[*main], pages, touched.as_ref())?;
    Ok(Some(out))
}

/// An object of the file as it was read.
enum Before {
    Stream(Dictionary, usize),
    Other(Object),
}

impl Before {
    fn of(object: &Object) -> Self {
        match object {
            Object::Stream(s) => Self::Stream(s.dict.clone(), s.content.len()),
            other => Self::Other(other.clone()),
        }
    }

    /// Whether `object` is still what it was.
    fn is(&self, object: &Object) -> bool {
        match (self, object) {
            (Self::Stream(dict, len), Object::Stream(s)) => {
                *dict == s.dict && *len == s.content.len()
            }
            (Self::Other(was), now) => was == now,
            _ => false,
        }
    }
}

/// Keeps what is written after the first `skip` bytes.
struct Tail {
    skip: usize,
    bytes: Vec<u8>,
}

impl Write for Tail {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let skipped = self.skip.min(buf.len());
        self.skip -= skipped;
        self.bytes.extend_from_slice(&buf[skipped..]);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
