//! Saving: writes the pages of an edited document as a new PDF.
//!
//! [`write_saved`] is what saving a document uses. Where the edit only adds to the file, it appends
//! the changes to it ([`append`]). Everything else, and everything the other functions here write,
//! is a full rewrite with lopdf, described from here on. The first source is the document being edited: its catalog (outline,
//! names, metadata, forms) is kept, and so is its encryption, re-applied with the original settings
//! and passwords ([`write_unprotected`] leaves it out). Pages of other sources are copied in with
//! everything they use.
//!
//! What the edit changes:
//!
//! - The page tree becomes one flat list of the pages kept, in their new
//!   order. Attributes pages inherited from intermediate nodes (resources,
//!   boxes, rotation) are copied onto each page first.
//! - A page's `/Rotate` gets the user's rotation added.
//! - Deleted pages are removed, with everything only they used. References
//!   to them elsewhere (outline entries, links) are left dangling, which
//!   readers treat as null: the entry stays, but goes nowhere.
//! - Page labels: each page keeps the label it had in its file. If the
//!   pages are all still there, in order, the file's `/PageLabels` stays as
//!   it is. Otherwise new ones are written (`PageLabels::of_pages`: pages
//!   of files without labels are numbered by position), or none if no page
//!   has a label of its own.
//! - Annotations added, changed or deleted are written into their pages'
//!   `/Annots` ([`AnnotChanges`]): new and changed ones as standard
//!   annotation objects with an appearance stream, so other programs show
//!   them as the app did. A deleted one takes its popup and the replies to
//!   it along. Everything else in `/Annots` (links, form fields, markup not
//!   touched) stays as it is.
//! - A page used twice gets a copy of its page dictionary (and of its
//!   annotations, except form fields), sharing contents and resources.
//!
//! lopdf drops the data of streams without a valid `/Length`, and gets the decryption of some
//! unusual files wrong, where hayro, which the apps render with, recovers. So every stream lopdf
//! loaded is checked against hayro's reading of the same object, and hayro's bytes win where they
//! differ: the saved file then shows what the app showed.
//!
//! Before returning, the result is read back and every page's content compared with the page it
//! came from. If anything differs, writing fails with [`WriteError::Unsupported`] rather than
//! producing a damaged file.
//!
//! [`save`] writes the result to a temporary file next to the target and renames it into place, so
//! the old file stays intact until the new one is complete, and a reader that mapped the old file
//! keeps its copy.

mod annots;
mod forms;
mod incremental;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, SaveOptions};
use specto_doc::{FieldId, FieldValue, PageAnnotChanges, PageRef, Rotation, SourceId};
use specto_source::hayro_syntax::object::{ObjectIdentifier, Stream as HayroStream};
use specto_source::{LabelStyle, PageLabels, PdfData};

pub use incremental::append;

/// Attributes a page can inherit from its ancestors in the page tree.
const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// A file to copy pages from.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    pub id: SourceId,
    pub data: &'a PdfData,
    pub password: Option<&'a str>,
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("no pages to write")]
    NoPages,
    #[error("a page comes from a source that wasn't given")]
    UnknownSource,
    #[error("the document is password protected")]
    PasswordRequired,
    #[error("page {0} isn't in the document")]
    MissingPage(u32),
    #[error("couldn't read the document: {0}")]
    Read(lopdf::Error),
    #[error("couldn't write the document: {0}")]
    Write(lopdf::Error),
    /// The result didn't match the original (see the crate documentation).
    #[error("this document can't be saved correctly yet")]
    Unsupported,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// What to do to the annotations of pages of the files, by source and page of it. Pages not named
/// keep theirs.
pub type AnnotChanges = HashMap<(SourceId, u32), PageAnnotChanges>;

/// Writes `pages`, taken from `sources`, as a new PDF.
///
/// The first source is the main document (see the crate documentation).
pub fn write(sources: &[Source], pages: &[PageRef]) -> Result<Vec<u8>, WriteError> {
    write_as(sources, pages, &AnnotChanges::new(), &[], true)
}

/// Like [`write()`], with the annotations changed as `annots` says.
///
/// `protected` is whether the main source's encryption is kept (see [`write_unprotected`]).
pub fn write_edited(
    sources: &[Source],
    pages: &[PageRef],
    annots: &AnnotChanges,
    protected: bool,
) -> Result<Vec<u8>, WriteError> {
    write_as(sources, pages, annots, &[], protected)
}

/// Like [`write_edited`], with `form` filled into the form of the main source: what each field
/// named holds now.
pub fn write_filled(
    sources: &[Source],
    pages: &[PageRef],
    annots: &AnnotChanges,
    form: &[(FieldId, FieldValue)],
    protected: bool,
) -> Result<Vec<u8>, WriteError> {
    write_as(sources, pages, annots, form, protected)
}

/// What saving writes: the main source's file with the changes appended where that is enough (see
/// [`append`]), and otherwise the document written anew like [`write_filled`] does, with the main
/// source's encryption kept.
pub fn write_saved(
    sources: &[Source],
    pages: &[PageRef],
    annots: &AnnotChanges,
    form: &[(FieldId, FieldValue)],
) -> Result<Vec<u8>, WriteError> {
    // Whatever keeps the changes from being appended, the rewrite gets its turn, and says what is
    // wrong if it can't be done either.
    if let Some(main) = sources.first()
        && let Ok(Some(out)) = append(main, pages, annots, form)
    {
        return Ok(out);
    }
    write_filled(sources, pages, annots, form, true)
}

/// Like [`write()`], but the result isn't encrypted, whether the main source is or not.
///
/// For handing the document to something that can't be given a password, such as a print service.
/// Not for saving, which would quietly take a document's protection away.
pub fn write_unprotected(sources: &[Source], pages: &[PageRef]) -> Result<Vec<u8>, WriteError> {
    write_as(sources, pages, &AnnotChanges::new(), &[], false)
}

fn write_as(
    sources: &[Source],
    pages: &[PageRef],
    annots: &AnnotChanges,
    form: &[(FieldId, FieldValue)],
    protected: bool,
) -> Result<Vec<u8>, WriteError> {
    let (main, others) = sources.split_first().ok_or(WriteError::NoPages)?;
    if pages.is_empty() {
        return Err(WriteError::NoPages);
    }
    let mut doc = load(main)?;
    let encryption = doc.encryption_state.take().filter(|_| protected);
    flatten_inherited(&mut doc)?;
    forms::apply(&mut doc, form)?;
    annotate(&mut doc, main.id, annots)?;
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(WriteError::Read)?;
    let root = doc
        .get_dictionary(catalog_id)
        .and_then(|c| c.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(WriteError::Read)?;

    // Page object of each (source, page).
    let mut page_ids: HashMap<(SourceId, u32), ObjectId> = doc
        .get_pages()
        .into_iter()
        .map(|(n, id)| ((main.id, n - 1), id))
        .collect();
    let main_pages: Vec<ObjectId> = page_ids.values().copied().collect();
    for source in others {
        if !pages.iter().any(|p| p.source == source.id) {
            continue;
        }
        for (n, id) in import(&mut doc, source, annots)? {
            page_ids.insert((source.id, n), id);
        }
    }

    // The new page list.
    let mut kids = Vec::with_capacity(pages.len());
    let mut used = HashSet::new();
    for p in pages {
        if p.source != main.id && !others.iter().any(|s| s.id == p.source) {
            return Err(WriteError::UnknownSource);
        }
        let id = *page_ids
            .get(&(p.source, p.page))
            .ok_or(WriteError::MissingPage(p.page))?;
        let id = if used.insert(id) {
            id
        } else {
            duplicate_page(&mut doc, id)?
        };
        let page = doc.get_dictionary_mut(id).map_err(WriteError::Read)?;
        page.set("Parent", root);
        if p.rotation != Rotation::None {
            let own = page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
            let total = Rotation::from_degrees(own).quarter_turns() + p.rotation.quarter_turns();
            page.set("Rotate", Rotation::from_quarter_turns(total).degrees());
        }
        kids.push(id);
    }

    // Pages of the main document that are gone. (Imported ones that aren't used were never
    // referenced, and are pruned.)
    for id in main_pages {
        if !used.contains(&id) {
            doc.objects.remove(&id);
        }
    }
    let tree = doc.get_dictionary_mut(root).map_err(WriteError::Read)?;
    tree.set(
        "Kids",
        kids.iter()
            .map(|&id| Object::Reference(id))
            .collect::<Vec<_>>(),
    );
    tree.set("Count", kids.len() as i64);
    tree.remove(b"Parent");

    let in_order = kids.len() == doc_page_count_before(&page_ids, main.id)
        && pages
            .iter()
            .enumerate()
            .all(|(i, p)| p.source == main.id && p.page as usize == i);
    if !in_order {
        let tree = page_labels(sources, pages).map(|l| label_tree(&l));
        if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
            match tree {
                Some(tree) => catalog.set("PageLabels", tree),
                None => {
                    catalog.remove(b"PageLabels");
                }
            }
        }
    }

    doc.prune_objects();
    let mut out = Vec::new();
    let encrypted = encryption.is_some();
    match encryption {
        Some(state) => {
            doc.encrypt(&state).map_err(WriteError::Write)?;
            doc.save_to(&mut out)?;
        }
        None => {
            let options = SaveOptions::builder()
                .use_object_streams(true)
                .use_xref_streams(true)
                .build();
            doc.save_with_options(&mut out, options)?;
        }
    }
    verify(&out, encrypted, sources, pages, None)?;
    Ok(out)
}

/// The names of a page's resources (fonts, images and so on) that resolve to something, by kind.
fn resource_names(r: &specto_source::hayro_syntax::page::Resources<'_>) -> Vec<Vec<Vec<u8>>> {
    [
        &r.ext_g_states,
        &r.fonts,
        &r.properties,
        &r.color_spaces,
        &r.x_objects,
        &r.patterns,
        &r.shadings,
    ]
    .iter()
    .map(|d| {
        let mut names: Vec<Vec<u8>> = d
            .keys()
            .filter(|k| {
                d.get::<specto_source::hayro_syntax::object::Object<'_>>(k.as_ref())
                    .is_some()
            })
            .map(|k| k.as_ref().to_vec())
            .collect();
        names.sort();
        names
    })
    .collect()
}

/// Reads `out` back and checks that it has the pages asked for, each with the same content and
/// boxes as the page it came from.
///
/// `encrypted` says whether `out` is, with the main source's passwords. With `only`, just the
/// pages at those positions are compared: for a file whose other pages are the bytes they were.
fn verify(
    out: &[u8],
    encrypted: bool,
    sources: &[Source],
    pages: &[PageRef],
    only: Option<&HashSet<usize>>,
) -> Result<(), WriteError> {
    let main = &sources[0];
    let password = main.password.filter(|_| encrypted);
    let written = specto_source::open_pdf(PdfData::from(out), password)
        .map_err(|_| WriteError::Unsupported)?;
    let mut originals = HashMap::new();
    for s in sources {
        if pages.iter().any(|p| p.source == s.id) {
            let pdf = specto_source::open_pdf(s.data.clone(), s.password)
                .map_err(|_| WriteError::Unsupported)?;
            originals.insert(s.id, pdf);
        }
    }
    let written_pages = written.pages();
    if written_pages.len() != pages.len() {
        return Err(WriteError::Unsupported);
    }
    for (i, p) in pages.iter().enumerate() {
        if only.is_some_and(|only| !only.contains(&i)) {
            continue;
        }
        let original = &originals[&p.source];
        let (Some(a), Some(b)) = (original.pages().get(p.page as usize), written_pages.get(i))
        else {
            return Err(WriteError::Unsupported);
        };
        let corners = |r: specto_source::hayro_syntax::object::Rect| [r.x0, r.y0, r.x1, r.y1];
        if a.page_stream() != b.page_stream()
            || corners(a.media_box()) != corners(b.media_box())
            || corners(a.crop_box()) != corners(b.crop_box())
            || resource_names(a.resources()) != resource_names(b.resources())
        {
            return Err(WriteError::Unsupported);
        }
    }
    Ok(())
}

/// The labels of `pages`, each keeping the label it has in its file.
fn page_labels(sources: &[Source], pages: &[PageRef]) -> Option<PageLabels> {
    let mut of_source: HashMap<SourceId, Option<PageLabels>> = HashMap::new();
    for s in sources {
        if pages.iter().any(|p| p.source == s.id) {
            let labels = specto_source::SourceDocument::open(s.data.clone(), s.password)
                .ok()
                .and_then(|d| d.page_labels())
                .filter(|l| !l.is_plain());
            of_source.insert(s.id, labels);
        }
    }
    PageLabels::of_pages(pages.iter().map(|p| {
        let labels = of_source.get(&p.source).and_then(Option::as_ref);
        (labels, p.page as usize)
    }))
}

/// `labels` as a `/PageLabels` number tree.
fn label_tree(labels: &PageLabels) -> Dictionary {
    let mut nums = Vec::new();
    for r in labels.ranges() {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"PageLabel".to_vec()));
        let style: Option<&[u8]> = match r.style {
            LabelStyle::Decimal => Some(b"D"),
            LabelStyle::UpperRoman => Some(b"R"),
            LabelStyle::LowerRoman => Some(b"r"),
            LabelStyle::UpperLetters => Some(b"A"),
            LabelStyle::LowerLetters => Some(b"a"),
            LabelStyle::None => None,
        };
        if let Some(s) = style {
            d.set("S", Object::Name(s.to_vec()));
        }
        if !r.prefix.is_empty() {
            d.set("P", text_string(&r.prefix));
        }
        if r.first != 1 && style.is_some() {
            d.set("St", i64::from(r.first));
        }
        nums.push(Object::Integer(r.start as i64));
        nums.push(Object::Dictionary(d));
    }
    let mut tree = Dictionary::new();
    tree.set("Nums", nums);
    tree
}

/// A PDF text string: ASCII as is, anything else as UTF-16BE.
fn text_string(s: &str) -> Object {
    let bytes = if s.is_ascii() {
        s.as_bytes().to_vec()
    } else {
        [0xFE, 0xFF]
            .into_iter()
            .chain(s.encode_utf16().flat_map(u16::to_be_bytes))
            .collect()
    };
    Object::String(bytes, lopdf::StringFormat::Literal)
}

/// How many pages the main document had.
fn doc_page_count_before(page_ids: &HashMap<(SourceId, u32), ObjectId>, main: SourceId) -> usize {
    page_ids.keys().filter(|(s, _)| *s == main).count()
}

/// Writes `pages` (see [`write()`]) to `path`, replacing it if it exists.
///
/// The data goes to a temporary file in the same directory first, which then takes the old file's
/// place, keeping its permissions.
pub fn save(path: &Path, sources: &[Source], pages: &[PageRef]) -> Result<(), WriteError> {
    save_edited(path, sources, pages, &AnnotChanges::new(), &[])
}

/// Like [`save`], with the annotations changed as `annots` says and `form` filled into the form.
pub fn save_edited(
    path: &Path,
    sources: &[Source],
    pages: &[PageRef],
    annots: &AnnotChanges,
    form: &[(FieldId, FieldValue)],
) -> Result<(), WriteError> {
    let data = write_saved(sources, pages, annots, form)?;
    replace_file(path, &data)?;
    Ok(())
}

/// Replaces the file at `path` with `data` atomically: other processes see either the old file or
/// the new one, and the old one is untouched until the new one is complete on disk. A symbolic link
/// is followed, so the link stays a link.
pub fn replace_file(path: &Path, data: &[u8]) -> io::Result<()> {
    let target = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        Err(e) => return Err(e),
    };
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file name"))?;
    let (temp, mut file) = temp_file(&dir, &name.to_string_lossy())?;
    let result = (|| {
        file.write_all(data)?;
        if let Ok(meta) = fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &target)?;
        // Make the rename itself durable.
        #[cfg(unix)]
        if let Ok(d) = File::open(&dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// A new file next to the target, named after it.
fn temp_file(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
        match File::options().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Parses (and decrypts) a source.
fn load(source: &Source) -> Result<Document, WriteError> {
    let options = LoadOptions {
        password: source.password.map(str::to_owned),
        ..LoadOptions::default()
    };
    let mut doc = Document::load_mem_with_options(source.data, options).map_err(|e| match e {
        lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword => WriteError::PasswordRequired,
        e => WriteError::Read(e),
    })?;
    if doc.is_encrypted() {
        // Still encrypted: the password didn't open it.
        return Err(WriteError::PasswordRequired);
    }
    repair_streams(&mut doc, source);
    Ok(doc)
}

/// Replaces the data of streams that lopdf read differently from hayro with hayro's (decrypted,
/// unfiltered) bytes, and drops `/Crypt` filters, since the document is encrypted with its default
/// filter when written. Returns how many streams were replaced.
fn repair_streams(doc: &mut Document, source: &Source) -> usize {
    let Ok(pdf) = specto_source::open_pdf(source.data.clone(), source.password) else {
        return 0;
    };
    let xref = pdf.xref();
    let mut replaced = 0;
    for (&(num, generation), obj) in doc.objects.iter_mut() {
        let Object::Stream(stream) = obj else {
            continue;
        };
        if stream.dict.has_type(b"ObjStm") || stream.dict.has_type(b"XRef") {
            continue;
        }
        let id = ObjectIdentifier::new(num, i32::from(generation));
        if let Some(theirs) = xref.get::<HayroStream<'_>>(id) {
            let raw = theirs.raw_data();
            if raw.as_ref() != stream.content.as_slice() {
                stream.set_content(raw.into_owned());
                replaced += 1;
            }
        }
        drop_crypt_filter(&mut stream.dict);
    }
    replaced
}

/// Removes `/Crypt` from a stream's filters, with its decode parameters.
fn drop_crypt_filter(dict: &mut Dictionary) {
    let Ok(Object::Array(filters)) = dict.get(b"Filter") else {
        if dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"Crypt") {
            dict.remove(b"Filter");
            dict.remove(b"DecodeParms");
        }
        return;
    };
    let Some(at) = filters
        .iter()
        .position(|f| f.as_name().ok() == Some(b"Crypt"))
    else {
        return;
    };
    let mut filters = filters.clone();
    filters.remove(at);
    if let Ok(Object::Array(params)) = dict.get(b"DecodeParms") {
        let mut params = params.clone();
        if at < params.len() {
            params.remove(at);
        }
        dict.set("DecodeParms", params);
    }
    dict.set("Filter", filters);
}

/// Copies inheritable attributes from ancestors onto every page that doesn't set them itself, so
/// pages can move to another parent.
fn flatten_inherited(doc: &mut Document) -> Result<(), WriteError> {
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for page in pages {
        let mut inherited = Dictionary::new();
        let mut seen = HashSet::new();
        let mut node = doc
            .get_dictionary(page)
            .map_err(WriteError::Read)?
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
        while let Some(id) = node {
            if !seen.insert(id) {
                break; // a cycle in a broken file
            }
            let Ok(d) = doc.get_dictionary(id) else { break };
            for k in INHERITABLE {
                if !inherited.has(k)
                    && let Ok(v) = d.get(k)
                {
                    inherited.set(k, v.clone());
                }
            }
            node = d.get(b"Parent").and_then(Object::as_reference).ok();
        }
        let p = doc.get_dictionary_mut(page).map_err(WriteError::Read)?;
        for (k, v) in inherited.iter() {
            if !p.has(k) {
                p.set(k.clone(), v.clone());
            }
        }
    }
    Ok(())
}

/// Copies the objects of another source into `doc`, renumbered past its own.
///
/// Returns the copied page objects by page index. Their catalog and page tree nodes are left
/// behind, and anything else they don't use is pruned later.
fn import(
    doc: &mut Document,
    source: &Source,
    annots: &AnnotChanges,
) -> Result<BTreeMap<u32, ObjectId>, WriteError> {
    let mut other = load(source)?;
    other.encryption_state = None;
    flatten_inherited(&mut other)?;
    annotate(&mut other, source.id, annots)?;
    other.renumber_objects_with(doc.max_id + 1);
    doc.max_id = doc.max_id.max(other.max_id);
    let pages: BTreeMap<u32, ObjectId> = other
        .get_pages()
        .into_iter()
        .map(|(n, id)| (n - 1, id))
        .collect();
    let catalog = other
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .ok();
    for (id, obj) in std::mem::take(&mut other.objects) {
        if Some(id) == catalog {
            continue;
        }
        if let Object::Dictionary(d) = &obj
            && d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Pages")
        {
            continue;
        }
        doc.objects.insert(id, obj);
    }
    Ok(pages)
}

/// Applies the annotation changes for pages of `source` to `doc`, that file as just loaded: before
/// its objects are renumbered, so the object numbers the changes name are the file's, and before a
/// page used twice is copied, so the copy has the same annotations.
fn annotate(doc: &mut Document, source: SourceId, annots: &AnnotChanges) -> Result<(), WriteError> {
    if !annots.keys().any(|(s, _)| *s == source) {
        return Ok(());
    }
    let pages = doc.get_pages();
    // In the order of the pages: new objects are numbered as they are made, and the same document
    // must be written the same every time.
    let mut of_source: Vec<(u32, &PageAnnotChanges)> = annots
        .iter()
        .filter(|((s, _), c)| *s == source && !c.is_empty())
        .map(|((_, page), changes)| (*page, changes))
        .collect();
    of_source.sort_unstable_by_key(|(page, _)| *page);
    for (page, changes) in of_source {
        // A page that isn't there isn't written either.
        if let Some(&id) = pages.get(&(page + 1)) {
            annots::apply(doc, id, changes)?;
        }
    }
    Ok(())
}

/// A second page object for a page used twice: a copy of the page dictionary, with copies of its
/// annotations (form fields left out, as a field can't sit on two pages).
///
/// Annotations that refer to each other on the page, such as markup and its popup, refer to each
/// other's copies.
fn duplicate_page(doc: &mut Document, id: ObjectId) -> Result<ObjectId, WriteError> {
    let mut page = doc.get_dictionary(id).map_err(WriteError::Read)?.clone();
    let new_id = doc.new_object_id();
    let annots: Vec<Object> = match page.get(b"Annots") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(Object::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut copied = Vec::new();
    // The id of each annotation's copy, by the id of the annotation.
    let mut ids = HashMap::new();
    for a in annots {
        let (old, dict) = match &a {
            Object::Reference(r) => (Some(*r), doc.get_dictionary(*r).ok().cloned()),
            Object::Dictionary(d) => (None, Some(d.clone())),
            _ => continue,
        };
        let Some(dict) = dict else { continue };
        if dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget") {
            continue;
        }
        let copy = doc.new_object_id();
        if let Some(old) = old {
            ids.insert(old, copy);
        }
        copied.push((copy, dict));
    }
    let mut copies = Vec::new();
    for (copy, mut dict) in copied {
        dict.set("P", new_id);
        for key in [&b"Popup"[..], b"Parent", b"IRT"] {
            if let Ok(Object::Reference(r)) = dict.get(key)
                && let Some(&to) = ids.get(r)
            {
                dict.set(key, to);
            }
        }
        doc.objects.insert(copy, Object::Dictionary(dict));
        copies.push(Object::Reference(copy));
    }
    if copies.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", copies);
    }
    doc.objects.insert(new_id, Object::Dictionary(page));
    Ok(new_id)
}
