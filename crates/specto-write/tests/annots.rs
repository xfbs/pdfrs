//! Annotations through a save: written, read back with our own import, and rendered with hayro,
//! which shows a file's annotations only through their appearance streams, as many other programs
//! do.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{
    Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream, dictionary,
};
use specto_annot::{AnnotKind, AnnotType, Annotation, Appearance, Color, MarkupStyle, import};
use specto_doc::{EditDoc, PageRef, SourceId};
use specto_geom::{PdfBox, PdfPoint, PdfQuad};
use specto_render::{Content, Document as _, HayroDocument, RenderRequest};
use specto_source::{PdfData, SourceDocument};
use specto_write::{AnnotChanges, Source, append, write_edited, write_saved};

const MAIN: SourceId = SourceId(0);
const YELLOW: Color = Color::new(1.0, 0.9, 0.0);
const RED: Color = Color::new(0.9, 0.1, 0.1);

/// Two pages of 600 by 800 with a black bar at x 100 to 300, y 700 to 730 (text, as far as a
/// highlight is concerned).
///
/// Page 1 has, in `/Annots`: a link, a square with an appearance (a green box at x 400 to 500, y
/// 100 to 200), its popup, and a reply to it.
fn fixture() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let mut kids = Vec::new();
    for _ in 0..2 {
        let content = b"0 g 100 700 200 30 re f".to_vec();
        let contents = doc.add_object(Stream::new(dictionary! {}, content));
        kids.push(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Contents" => contents,
        }));
    }
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link",
        "Rect" => vec![0.into(), 0.into(), 50.into(), 50.into()],
        "Dest" => vec![kids[1].into(), "Fit".into()],
    });
    let look = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![400.into(), 100.into(), 500.into(), 200.into()],
        },
        b"0 1 0 rg 400 100 100 100 re f".to_vec(),
    ));
    let square = doc.new_object_id();
    let popup = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Popup", "Parent" => square,
        "Rect" => vec![500.into(), 100.into(), 600.into(), 200.into()],
    });
    doc.objects.insert(
        square,
        dictionary! {
            "Type" => "Annot", "Subtype" => "Square", "Popup" => popup,
            "Rect" => vec![400.into(), 100.into(), 500.into(), 200.into()],
            "C" => vec![0.into(), 1.into(), 0.into()],
            "Contents" => Object::string_literal("A box"),
            "Vendor" => Object::string_literal("kept"),
            "AP" => dictionary! { "N" => look },
        }
        .into(),
    );
    let reply = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "IRT" => square,
        "Rect" => vec![400.into(), 100.into(), 420.into(), 120.into()],
        "Contents" => Object::string_literal("Agreed"),
    });
    doc.get_dictionary_mut(kids[0]).unwrap().set(
        "Annots",
        vec![link.into(), square.into(), popup.into(), reply.into()],
    );
    doc.objects.insert(
        pages,
        dictionary! {
            "Type" => "Pages", "Count" => 2,
            "Kids" => kids.iter().map(|&k| Object::Reference(k)).collect::<Vec<_>>(),
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Resources" => dictionary! {},
        }
        .into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal(b"0123456789abcdef".to_vec()),
            Object::string_literal(b"0123456789abcdef".to_vec()),
        ],
    );
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn encrypted(data: &[u8], password: &str) -> Vec<u8> {
    encrypted_allowing(data, password, Permissions::all())
}

fn encrypted_allowing(data: &[u8], password: &str, permissions: Permissions) -> Vec<u8> {
    let mut doc = Document::load_mem(data).unwrap();
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let key = [7u8; 32];
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "owner",
        user_password: password,
        permissions,
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// A quad over x `x0` to `x1` of the bar, bottom left first.
fn over_bar(x0: f64, x1: f64) -> PdfQuad {
    PdfQuad::new([
        PdfPoint::new(x0, 700.0),
        PdfPoint::new(x1, 700.0),
        PdfPoint::new(x1, 730.0),
        PdfPoint::new(x0, 730.0),
    ])
}

fn markup(style: MarkupStyle, x0: f64, x1: f64, color: Color) -> Annotation {
    Annotation::text_markup(style, vec![over_bar(x0, x1)], color).unwrap()
}

/// What saving does with `edit`'s annotations.
fn changes(edit: &EditDoc) -> AnnotChanges {
    edit.pages()
        .iter()
        .map(|p| {
            (
                (p.source, p.page),
                edit.annotation_changes(p.source, p.page),
            )
        })
        .collect()
}

fn write(data: &[u8], password: Option<&str>, edit: &EditDoc) -> Vec<u8> {
    let data = PdfData::from(data.to_vec());
    let source = Source {
        id: MAIN,
        data: &data,
        password,
    };
    let out = write_edited(&[source], edit.pages(), &changes(edit), true).expect("it saves");
    qpdf_check(&out, password);
    out
}

/// The color at PDF point (`x`, `y`) of `page`, rendered at one pixel a point, with or without the
/// annotations.
fn pixel(
    data: &[u8],
    password: Option<&str>,
    page: usize,
    x: u32,
    y: u32,
    annots: bool,
) -> [u8; 3] {
    let doc = HayroDocument::open(data.to_vec(), password).expect("the file opens");
    let mut req = RenderRequest::page(page, 1.0);
    if !annots {
        req.content = Content::PageOnly;
    }
    let bitmap = doc.render(&req).unwrap();
    let at = ((799 - y) * bitmap.width + x) as usize * 4;
    [bitmap.data[at], bitmap.data[at + 1], bitmap.data[at + 2]]
}

fn imported(data: &[u8], password: Option<&str>, page: usize) -> Vec<Annotation> {
    let doc = SourceDocument::open(data.to_vec(), password).expect("the file opens");
    import(&doc, page)
}

fn imported_arcs(data: &[u8], page: usize) -> Vec<Arc<Annotation>> {
    imported(data, None, page)
        .into_iter()
        .map(Arc::new)
        .collect()
}

/// `qpdf --check` passes, if qpdf is installed.
fn qpdf_check(data: &[u8], password: Option<&str>) {
    let path = std::env::temp_dir().join(format!(
        "specto-annots-{}-{:x}.pdf",
        std::process::id(),
        data.len() ^ data.iter().take(4096).map(|&b| b as usize).sum::<usize>()
    ));
    std::fs::write(&path, data).unwrap();
    let mut cmd = Command::new("qpdf");
    if let Some(p) = password {
        cmd.arg(format!("--password={p}"));
    }
    let result = cmd.arg("--check").arg(&path).output();
    std::fs::remove_file(&path).unwrap();
    let Ok(out) = result else {
        eprintln!("qpdf not installed; skipping the check");
        return;
    };
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "qpdf --check: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn new_markup_is_saved_and_shows() {
    let original = fixture();
    let mut edit = EditDoc::from_source(MAIN, 2);
    let mut highlight = markup(MarkupStyle::Highlight, 90.0, 200.0, YELLOW);
    highlight.contents = Some("Größe".to_owned());
    highlight.modified = specto_annot::Date::parse(b"D:20261115190000+01'00'");
    edit.add_annotation(MAIN, 1, highlight);
    edit.add_annotation(MAIN, 1, markup(MarkupStyle::Underline, 350.0, 450.0, RED));
    edit.add_annotation(MAIN, 1, markup(MarkupStyle::StrikeOut, 350.0, 450.0, RED));
    let out = write(&original, None, &edit);

    // Read back: what was put in, now as annotations of the file.
    let annots = imported(&out, None, 1);
    let types: Vec<AnnotType> = annots.iter().map(Annotation::annot_type).collect();
    assert_eq!(
        types,
        [
            AnnotType::Highlight,
            AnnotType::Underline,
            AnnotType::StrikeOut
        ]
    );
    let h = &annots[0];
    assert_eq!(h.color, Some(YELLOW));
    assert_eq!(h.contents.as_deref(), Some("Größe"));
    assert_eq!(
        h.modified.map(|d| d.to_string()).as_deref(),
        Some("2026-11-15T19:00:00+01:00")
    );
    // Recognized as a look written here.
    assert_eq!(h.appearance, Appearance::Own);
    assert!(h.flags.is_visible());
    assert_eq!(
        h.rect,
        PdfBox::new(PdfPoint::new(90.0, 700.0), PdfPoint::new(200.0, 730.0))
    );
    let AnnotKind::TextMarkup { quads, .. } = &h.kind else {
        panic!("not text markup");
    };
    assert_eq!(quads, &[over_bar(90.0, 200.0)]);
    // Page 1 keeps what it had.
    assert_eq!(imported(&out, None, 0).len(), 2);

    // Rendered: yellow beside the bar, and the bar still black under the highlight (it darkens, it
    // doesn't cover). Nothing without annotations.
    let at = |x, y, annots| pixel(&out, None, 1, x, y, annots);
    let [r, g, b] = at(95, 715, true);
    assert!(r > 240 && g > 215 && b < 30, "beside the bar: {r} {g} {b}");
    assert_eq!(at(150, 715, true), [0, 0, 0]);
    assert_eq!(at(95, 715, false), [255, 255, 255]);
    assert_eq!(at(250, 715, true), [0, 0, 0]);
    // The underline near the bottom of its quad, the strike-out further up, white between and
    // above.
    let red = |[r, g, b]: [u8; 3]| r > 200 && g < 60 && b < 60;
    assert!(
        red(at(400, 702, true)),
        "underline: {:?}",
        at(400, 702, true)
    );
    assert!(
        red(at(400, 712, true)),
        "strike-out: {:?}",
        at(400, 712, true)
    );
    assert_eq!(at(400, 707, true), [255, 255, 255]);
    assert_eq!(at(400, 722, true), [255, 255, 255]);
}

#[test]
fn deleting_one_of_the_files() {
    let original = fixture();
    let on_page = imported_arcs(&original, 0);
    assert_eq!(on_page.len(), 2, "the square and the reply to it");
    assert_eq!(pixel(&original, None, 0, 450, 150, true), [0, 255, 0]);

    let mut edit = EditDoc::from_source(MAIN, 2);
    let (id, square) = edit.annotations(MAIN, 0, &on_page)[0].clone();
    assert!(edit.remove_annotation(id, &square));
    let out = write(&original, None, &edit);

    // Gone, with its popup and the reply. The link stays.
    assert!(imported(&out, None, 0).is_empty());
    assert_eq!(pixel(&out, None, 0, 450, 150, true), [255, 255, 255]);
    let doc = SourceDocument::open(out.clone(), None).unwrap();
    assert_eq!(doc.links(0).len(), 1);
    let written = Document::load_mem(&out).unwrap();
    let page = written.get_pages()[&1];
    let annots = written
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap();
    assert_eq!(annots.as_array().unwrap().len(), 1);
}

#[test]
fn changing_one_of_the_files() {
    let original = fixture();
    let on_page = imported_arcs(&original, 0);
    let mut edit = EditDoc::from_source(MAIN, 2);
    let (id, square) = edit.annotations(MAIN, 0, &on_page)[0].clone();
    let mut changed = (*square).clone();
    changed.contents = Some("Another box".to_owned());
    changed.color = Some(RED);
    assert!(edit.change_annotation(id, changed));
    let out = write(&original, None, &edit);

    let annots = imported(&out, None, 0);
    assert_eq!(annots.len(), 2);
    assert_eq!(annots[0].contents.as_deref(), Some("Another box"));
    assert_eq!(annots[0].color, Some(RED));
    // Only its values changed, so it looks as it did, and is the same object: its popup and the
    // reply still belong to it, and what this program doesn't know about is kept.
    assert_eq!(annots[0].appearance, Appearance::Original);
    assert_eq!(pixel(&out, None, 0, 450, 150, true), [0, 255, 0]);
    let written = Document::load_mem(&out).unwrap();
    let page = written.get_pages()[&1];
    let entries = written
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap();
    let entries = entries.as_array().unwrap();
    assert_eq!(entries.len(), 4);
    let square_id = entries[1].as_reference().unwrap();
    let dict = written.get_dictionary(square_id).unwrap();
    assert_eq!(
        dict.get(b"Popup").unwrap().as_reference().unwrap(),
        entries[2].as_reference().unwrap()
    );
    assert!(dict.has(b"Vendor"));
    let reply = written
        .get_dictionary(entries[3].as_reference().unwrap())
        .unwrap();
    assert_eq!(
        reply.get(b"IRT").unwrap().as_reference().unwrap(),
        square_id
    );
}

#[test]
fn what_an_encrypted_file_allows() {
    let plain = SourceDocument::open(fixture(), None).unwrap();
    assert_eq!(plain.permissions(), None);

    let all = SourceDocument::open(encrypted(&fixture(), "secret"), Some("secret")).unwrap();
    let allowed = all.permissions().unwrap();
    assert!(allowed.printing && allowed.changing && allowed.copying && allowed.annotating);

    // Without a password to open it, and only printing and annotating allowed.
    let some = Permissions::PRINTABLE | Permissions::ANNOTABLE;
    let some = SourceDocument::open(encrypted_allowing(&fixture(), "", some), None).unwrap();
    assert_eq!(
        some.permissions(),
        Some(specto_source::Permissions {
            printing: true,
            changing: false,
            copying: false,
            annotating: true,
        })
    );
}

/// `data` written with a cross-reference stream and its objects in object streams.
fn compact(data: &[u8]) -> Vec<u8> {
    let mut doc = Document::load_mem(data).unwrap();
    let options = lopdf::SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .build();
    let mut out = Vec::new();
    doc.save_with_options(&mut out, options).unwrap();
    out
}

/// What [`append`] makes of `edit`, if it takes it.
fn appended(data: &[u8], password: Option<&str>, edit: &EditDoc) -> Option<Vec<u8>> {
    let data = PdfData::from(data.to_vec());
    let source = Source {
        id: MAIN,
        data: &data,
        password,
    };
    append(&source, edit.pages(), &changes(edit), &[]).expect("no error")
}

#[test]
fn new_annotations_are_appended_to_the_file() {
    let files = [
        (fixture(), None),
        (compact(&fixture()), None),
        (encrypted(&fixture(), "secret"), Some("secret")),
        (encrypted(&fixture(), ""), None),
    ];
    for (original, password) in files {
        let mut edit = EditDoc::from_source(MAIN, 2);
        let mut highlight = markup(MarkupStyle::Highlight, 90.0, 200.0, YELLOW);
        highlight.contents = Some("Größe".to_owned());
        edit.add_annotation(MAIN, 1, highlight);
        let out = appended(&original, password, &edit).expect("only something new");
        qpdf_check(&out, password);

        // The file as it was, and a little after it.
        assert!(out.starts_with(&original));
        assert!(out.len() < original.len() + 2000, "{}", out.len());
        assert_eq!(
            SourceDocument::open(out.clone(), None).is_err(),
            password.is_some(),
            "protected as before"
        );
        let annots = imported(&out, password, 1);
        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0].annot_type(), AnnotType::Highlight);
        assert_eq!(annots[0].contents.as_deref(), Some("Größe"));
        assert_eq!(annots[0].appearance, Appearance::Own);
        let [r, g, b] = pixel(&out, password, 1, 95, 715, true);
        assert!(r > 240 && g > 215 && b < 30, "the highlight: {r} {g} {b}");
        assert_eq!(imported(&out, password, 0).len(), 2, "page 1 as it was");

        // And once more, onto what was appended.
        let mut edit = EditDoc::from_source(MAIN, 2);
        edit.add_annotation(MAIN, 1, markup(MarkupStyle::Underline, 350.0, 450.0, RED));
        edit.add_annotation(MAIN, 0, markup(MarkupStyle::StrikeOut, 350.0, 450.0, RED));
        let again = appended(&out, password, &edit).expect("only something new");
        qpdf_check(&again, password);
        assert!(again.starts_with(&out));
        assert_eq!(imported(&again, password, 1).len(), 2);
        assert_eq!(imported(&again, password, 0).len(), 3);
    }
}

#[test]
fn what_takes_something_away_is_not_appended() {
    let original = fixture();
    let data = PdfData::from(original.clone());
    let source = Source {
        id: MAIN,
        data: &data,
        password: None,
    };
    let added = || {
        let mut edit = EditDoc::from_source(MAIN, 2);
        edit.add_annotation(MAIN, 1, markup(MarkupStyle::Highlight, 90.0, 200.0, YELLOW));
        edit
    };
    assert!(appended(&original, None, &added()).is_some());

    // Nothing to write.
    assert!(appended(&original, None, &EditDoc::from_source(MAIN, 2)).is_none());
    // A page turned, moved or deleted.
    let mut edit = added();
    edit.rotate(&[0], specto_doc::Rotation::Cw90);
    assert!(appended(&original, None, &edit).is_none());
    let mut edit = added();
    edit.move_pages(&[1], 0);
    assert!(appended(&original, None, &edit).is_none());
    let mut edit = added();
    edit.delete(&[0]);
    assert!(appended(&original, None, &edit).is_none());

    // An annotation of the file deleted: it must not stay in the file for someone to dig out.
    let on_page = imported_arcs(&original, 0);
    let mut edit = added();
    let id = edit.annotations(MAIN, 0, &on_page)[0].0;
    assert!(edit.remove_annotation(id, &on_page[0]));
    assert!(appended(&original, None, &edit).is_none());
    let out = write_saved(&[source], edit.pages(), &changes(&edit), &[]).expect("it saves");
    assert!(!out.starts_with(&original));
    let needle = b"/Subtype/Square";
    assert!(original.windows(needle.len()).any(|w| w == needle));
    assert!(!out.windows(needle.len()).any(|w| w == needle));

    // Saving appends where it can.
    let edit = added();
    let out = write_saved(&[source], edit.pages(), &changes(&edit), &[]).expect("it saves");
    assert!(out.starts_with(&original));
}

#[test]
fn encrypted_and_on_a_page_used_twice() {
    let original = encrypted(&fixture(), "secret");
    let mut edit = EditDoc::from_source(MAIN, 2);
    let mut highlight = markup(MarkupStyle::Highlight, 90.0, 200.0, YELLOW);
    highlight.contents = Some("A note".to_owned());
    edit.add_annotation(MAIN, 1, highlight);
    // Page 2 a second time, at the end.
    edit.insert(2, vec![PageRef::new(MAIN, 1)]);
    let out = write(&original, Some("secret"), &edit);

    assert!(
        SourceDocument::open(out.clone(), None).is_err(),
        "still protected"
    );
    for page in [1, 2] {
        let annots = imported(&out, Some("secret"), page);
        assert_eq!(annots.len(), 1, "page {page}");
        assert_eq!(annots[0].contents.as_deref(), Some("A note"));
        let [r, g, b] = pixel(&out, Some("secret"), page, 95, 715, true);
        assert!(r > 240 && g > 215 && b < 30, "page {page}: {r} {g} {b}");
    }
}

#[test]
fn an_annotation_that_isnt_where_it_was_read_is_not_touched() {
    let original = fixture();
    let on_page = imported_arcs(&original, 0);
    let (_, square) = EditDoc::from_source(MAIN, 2).annotations(MAIN, 0, &on_page)[0].clone();
    let data = PdfData::from(original);
    let source = Source {
        id: MAIN,
        data: &data,
        password: None,
    };
    let pages = [PageRef::new(MAIN, 0)];
    let removing = |origin: specto_annot::Origin| {
        let changes = specto_doc::PageAnnotChanges {
            removed: vec![origin],
            ..Default::default()
        };
        write_edited(
            &[source],
            &pages,
            &AnnotChanges::from([((MAIN, 0), changes)]),
            true,
        )
    };
    let origin = square.origin.unwrap();
    assert!(removing(origin).is_ok());
    // Another object at that place, a place that holds something else (the link, entry 0, is
    // another object too), a place beyond the end, and one said to be written into the array that
    // is a reference.
    let (number, generation) = origin.object.unwrap();
    for wrong in [
        specto_annot::Origin {
            object: Some((number + 1, generation)),
            ..origin
        },
        specto_annot::Origin { index: 0, ..origin },
        specto_annot::Origin { index: 9, ..origin },
        specto_annot::Origin {
            object: None,
            ..origin
        },
    ] {
        assert!(
            matches!(removing(wrong), Err(specto_write::WriteError::Unsupported)),
            "{wrong:?}"
        );
    }
}

/// Every annotation of the corpus, deleted on its own: the page is written without it, as the apps
/// do to show the page then, and read back it has one annotation less and the others as before.
///
/// Prints what couldn't be done, and fails only on a result that is wrong.
#[test]
#[ignore = "needs corpus/files (run corpus/fetch.sh)"]
fn corpus_annotations_can_be_deleted() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/files");
    let mut stack = vec![dir];
    let mut paths = Vec::new();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "pdf") {
                paths.push(p);
            }
        }
    }
    paths.sort();
    let (mut annotations, mut deleted) = (0, 0);
    let (mut refused, mut wrong, mut panics) = (Vec::new(), Vec::new(), Vec::new());
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let data = PdfData::open(path).unwrap();
        let Ok(doc) = SourceDocument::open(data.clone(), None) else {
            continue;
        };
        let source = Source {
            id: MAIN,
            data: &data,
            password: None,
        };
        for page in 0..doc.page_count().min(50) as u32 {
            let before = import(&doc, page as usize);
            for (i, a) in before.iter().enumerate() {
                annotations += 1;
                let what = format!("{name} page {} annotation {i}", page + 1);
                let changes = specto_doc::PageAnnotChanges {
                    removed: vec![a.origin.unwrap()],
                    ..Default::default()
                };
                let r = catch_unwind(AssertUnwindSafe(|| {
                    write_edited(
                        &[source],
                        &[PageRef::new(MAIN, page)],
                        &AnnotChanges::from([((MAIN, page), changes)]),
                        false,
                    )
                }));
                let out = match r {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => {
                        refused.push(format!("{what}: {e}"));
                        continue;
                    }
                    Err(_) => {
                        panics.push(what);
                        continue;
                    }
                };
                let Ok(written) = SourceDocument::open(out, None) else {
                    wrong.push(format!("{what}: the result doesn't open"));
                    continue;
                };
                // What is left, by what it is and where: the one deleted is missing, and with it
                // the replies to it at most. (Nearly where: a rewritten file has its numbers in
                // single precision.)
                let same = |a: &Annotation, b: &Annotation| {
                    let corners = |r: PdfBox| [r.min.x, r.min.y, r.max.x, r.max.y];
                    a.annot_type() == b.annot_type()
                        && corners(a.rect)
                            .iter()
                            .zip(corners(b.rect))
                            .all(|(x, y)| (x - y).abs() <= 0.01 + x.abs() * 1e-5)
                };
                let after = import(&written, 0);
                let mut expected = before.clone();
                expected.remove(i);
                let is_rest = after.len() <= expected.len() && {
                    let mut rest = expected.iter();
                    after.iter().all(|a| rest.any(|e| same(e, a)))
                };
                if is_rest {
                    deleted += 1;
                } else {
                    wrong.push(format!("{what}: {} left of {}", after.len(), before.len()));
                }
            }
        }
    }
    println!("{annotations} annotations, {deleted} deleted");
    for (title, list) in [
        ("refused", &refused),
        ("wrong", &wrong),
        ("panicked", &panics),
    ] {
        println!("{title}: {}", list.len());
        for l in list.iter().take(40) {
            println!("  {l}");
        }
    }
    assert!(wrong.is_empty() && panics.is_empty());
}

#[test]
fn drawings_and_shapes_are_saved_and_show() {
    let original = fixture();
    let mut edit = EditDoc::from_source(MAIN, 2);
    let point = PdfPoint::new;
    let area = |x0, y0, x1, y1| PdfBox::new(point(x0, y0), point(x1, y1));
    let with = |kind: AnnotKind, rect: PdfBox, width: f64| {
        let mut a = Annotation::new(kind, rect);
        a.color = Some(RED);
        a.line_width = width;
        a
    };
    // A stroke in a right angle, a rectangle with a green inside, an ellipse, a line, and a dot.
    // (Points every few units, as a pointer leaves them: the curve through them then keeps to the
    // legs.)
    let up = (0..=10).map(|i| point(100.0, 100.0 + 10.0 * f64::from(i)));
    let right = (1..=10).map(|i| point(100.0 + 10.0 * f64::from(i), 200.0));
    let stroke: Vec<PdfPoint> = up.chain(right).collect();
    edit.add_annotation(
        MAIN,
        1,
        with(
            AnnotKind::Ink {
                strokes: vec![stroke.clone()],
            },
            area(98.0, 98.0, 202.0, 202.0),
            4.0,
        ),
    );
    let green = Color::new(0.0, 1.0, 0.0);
    edit.add_annotation(
        MAIN,
        1,
        with(
            AnnotKind::Square { fill: Some(green) },
            area(300.0, 100.0, 400.0, 200.0),
            6.0,
        ),
    );
    edit.add_annotation(
        MAIN,
        1,
        with(
            AnnotKind::Circle { fill: None },
            area(300.0, 300.0, 500.0, 400.0),
            6.0,
        ),
    );
    let (from, to) = (point(100.0, 500.0), point(300.0, 500.0));
    edit.add_annotation(
        MAIN,
        1,
        with(
            AnnotKind::Line { from, to },
            area(98.0, 498.0, 302.0, 502.0),
            4.0,
        ),
    );
    edit.add_annotation(
        MAIN,
        1,
        with(
            AnnotKind::Ink {
                strokes: vec![vec![point(500.0, 600.0)]],
            },
            area(495.0, 595.0, 505.0, 605.0),
            10.0,
        ),
    );
    let out = write(&original, None, &edit);

    let annots = imported(&out, None, 1);
    let types: Vec<AnnotType> = annots.iter().map(Annotation::annot_type).collect();
    assert_eq!(
        types,
        [
            AnnotType::Ink,
            AnnotType::Square,
            AnnotType::Circle,
            AnnotType::Line,
            AnnotType::Ink
        ]
    );
    assert_eq!(
        annots[0].kind,
        AnnotKind::Ink {
            strokes: vec![stroke]
        }
    );
    assert_eq!(annots[0].line_width, 4.0);
    assert_eq!(annots[0].color, Some(RED));
    assert_eq!(annots[1].kind, AnnotKind::Square { fill: Some(green) });
    // Its area as it went in: it doesn't grow with every save.
    assert_eq!(annots[1].rect, area(300.0, 100.0, 400.0, 200.0));
    assert_eq!(annots[2].rect, area(300.0, 300.0, 500.0, 400.0));
    assert_eq!(annots[3].kind, AnnotKind::Line { from, to });
    assert!(annots.iter().all(|a| a.appearance == Appearance::Own));
    assert!(annots.iter().all(|a| a.is_adjustable()));

    let at = |x, y| pixel(&out, None, 1, x, y, true);
    let red = |[r, g, b]: [u8; 3]| r > 180 && g < 80 && b < 80;
    let white = [255, 255, 255];
    // The stroke: on its two legs, round at the corner, nothing inside.
    assert!(red(at(100, 150)) && red(at(150, 200)), "the stroke");
    assert_eq!(at(150, 150), white);
    // The rectangle: its line just inside its area, green within.
    assert!(
        red(at(303, 150)) && red(at(350, 197)),
        "the rectangle's line"
    );
    assert_eq!(at(350, 150), [0, 255, 0]);
    assert_eq!(at(296, 150), white);
    // The ellipse: its line at the ends of its axes, nothing in the middle or in the corners of its
    // area.
    assert!(red(at(303, 350)) && red(at(400, 397)), "the ellipse's line");
    assert_eq!(at(400, 350), white);
    assert_eq!(at(305, 305), white);
    // The line, with round ends reaching half its width beyond.
    assert!(red(at(200, 500)) && red(at(99, 500)), "the line");
    assert_eq!(at(200, 505), white);
    // The dot.
    assert!(red(at(500, 600)) && red(at(503, 600)), "the dot");
    assert_eq!(at(508, 600), white);
}

#[test]
fn notes_and_text_boxes_are_saved_and_show() {
    let original = fixture();
    let mut edit = EditDoc::from_source(MAIN, 2);
    let blue = Color::new(0.0, 0.0, 1.0);
    let mut note = Annotation::note(PdfPoint::new(50.0, 600.0), YELLOW);
    note.contents = Some("Check this (again)".to_owned());
    edit.add_annotation(MAIN, 1, note.clone());
    // Wide enough for "Hello, world" in 20 points, not for the rest of it.
    let text = "Hello, world of (bracketed) text\nGrüße";
    let bold_serif = specto_annot::TextFont {
        family: specto_annot::FontFamily::Serif,
        bold: true,
        italic: false,
    };
    let text_box = Annotation::text_box(
        PdfPoint::new(100.0, 500.0),
        150.0,
        text,
        (20.0, bold_serif),
        blue,
    );
    assert!(text_box.rect.height() > 3.0 * 20.0, "three lines at least");
    edit.add_annotation(MAIN, 1, text_box.clone());
    let out = write(&original, None, &edit);

    let annots = imported(&out, None, 1);
    let types: Vec<AnnotType> = annots.iter().map(Annotation::annot_type).collect();
    assert_eq!(types, [AnnotType::Note, AnnotType::FreeText]);
    assert_eq!(annots[0].contents.as_deref(), Some("Check this (again)"));
    assert_eq!(annots[0].color, Some(YELLOW));
    assert_eq!(annots[0].rect, note.rect);
    assert_eq!(
        annots[1].kind,
        AnnotKind::FreeText {
            size: 20.0,
            font: bold_serif
        }
    );
    assert_eq!(annots[1].contents.as_deref(), Some(text));
    assert_eq!(annots[1].color, Some(blue));
    assert_eq!(annots[1].rect, text_box.rect);
    // Both to be moved and changed another day.
    assert!(
        annots
            .iter()
            .all(|a| a.appearance == Appearance::Own && a.is_adjustable())
    );
    assert!(annots[1].is_resizable() && !annots[0].is_resizable());

    // The note's mark is there, in its color.
    let [r, g, b] = pixel(&out, None, 1, 65, 593, true);
    assert!(r > 200 && g > 180 && b < 80, "the note: {r} {g} {b}");
    // The text box: blue ink on its lines, each within the box.
    let ink = |x0: u32, x1: u32, y0: u32, y1: u32| {
        (x0..x1)
            .flat_map(|x| (y0..y1).map(move |y| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b] = pixel_of(&out, x, y);
                b > 150 && r < 100 && g < 100
            })
            .count()
    };
    let top = text_box.rect.max.y as u32;
    assert!(ink(100, 250, top - 24, top) > 50, "no first line");
    assert!(ink(100, 250, top - 48, top - 24) > 50, "no second line");
    assert_eq!(ink(252, 400, top - 100, top), 0, "text beyond the box");
    assert_eq!(pixel(&out, None, 1, 175, top + 10, true), [255, 255, 255]);
}

/// The color at a point of page 2 of an unencrypted file, as [`pixel`] gives it, rendering the page
/// once however often it is asked.
fn pixel_of(data: &[u8], x: u32, y: u32) -> [u8; 3] {
    use std::cell::RefCell;
    thread_local! {
        static PAGE: RefCell<Option<(usize, specto_render::Bitmap)>> = const { RefCell::new(None) };
    }
    PAGE.with_borrow_mut(|page| {
        let key = data.as_ptr() as usize;
        if page.as_ref().is_none_or(|(k, _)| *k != key) {
            let doc = HayroDocument::open(data.to_vec(), None).expect("the file opens");
            let bitmap = doc.render(&RenderRequest::page(1, 1.0)).unwrap();
            *page = Some((key, bitmap));
        }
        let bitmap = &page.as_ref().unwrap().1;
        let at = ((799 - y) * bitmap.width + x) as usize * 4;
        [bitmap.data[at], bitmap.data[at + 1], bitmap.data[at + 2]]
    })
}

#[test]
fn stamps_are_saved_show_and_move() {
    use specto_annot::{StampArt, StampImage};

    // A picture of 4 by 2 pixels: red on the left, nothing on the right.
    let mut rgba = Vec::new();
    for _ in 0..2 {
        rgba.extend([255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0]);
    }
    let image = Arc::new(StampImage {
        width: 4,
        height: 2,
        rgba,
    });
    let original = fixture();
    let mut edit = EditDoc::from_source(MAIN, 2);
    // 200 wide, so 100 high, around (300, 400).
    let stamp = Annotation::stamp(PdfPoint::new(300.0, 400.0), 200.0, image);
    assert_eq!(
        stamp.rect,
        PdfBox::new(PdfPoint::new(200.0, 350.0), PdfPoint::new(400.0, 450.0))
    );
    edit.add_annotation(MAIN, 1, stamp.clone());
    let out = write(&original, None, &edit);

    // Red where the picture is, the page where it is transparent.
    assert_eq!(pixel(&out, None, 1, 250, 400, true), [255, 0, 0]);
    assert_eq!(pixel(&out, None, 1, 350, 400, true), [255, 255, 255]);
    assert_eq!(pixel(&out, None, 1, 190, 400, true), [255, 255, 255]);

    // Read again: a stamp whose picture is the file's, still to be moved.
    let on_page: Vec<Arc<Annotation>> = imported(&out, None, 1).into_iter().map(Arc::new).collect();
    assert_eq!(on_page.len(), 1);
    assert_eq!(on_page[0].kind, AnnotKind::Stamp(StampArt::InFile));
    assert_eq!(on_page[0].rect, stamp.rect);
    assert!(on_page[0].is_adjustable() && on_page[0].keeps_proportions());

    // Moved down by 200 and made half the size: the picture goes along.
    let mut edit = EditDoc::from_source(MAIN, 2);
    let (id, a) = edit.annotations(MAIN, 1, &on_page)[0].clone();
    let moved =
        a.transformed(|p| PdfPoint::new(200.0 + (p.x - 200.0) / 2.0, 150.0 + (p.y - 350.0) / 2.0));
    assert_eq!(moved.appearance, Appearance::Own);
    assert!(edit.change_annotation(id, moved));
    let again = write(&out, None, &edit);
    assert_eq!(pixel(&again, None, 1, 225, 175, true), [255, 0, 0]);
    assert_eq!(pixel(&again, None, 1, 275, 175, true), [255, 255, 255]);
    assert_eq!(pixel(&again, None, 1, 250, 400, true), [255, 255, 255]);
    let after = imported(&again, None, 1);
    assert_eq!(after[0].kind, AnnotKind::Stamp(StampArt::InFile));
    assert!(after[0].is_adjustable());
}

#[test]
fn a_drawing_placed_from_elsewhere() {
    // Two strokes in a box of 100 by 50, y down: a line along the top and one down the left. Placed
    // 200 wide around (300, 400).
    let strokes = vec![
        vec![(0.0, 0.0), (100.0, 0.0)],
        vec![(0.0, 0.0), (0.0, 50.0)],
    ];
    let a = Annotation::drawing(
        PdfPoint::new(300.0, 400.0),
        200.0,
        &strokes,
        (100.0, 50.0),
        RED,
        2.0,
    )
    .expect("there are strokes");
    let AnnotKind::Ink { strokes: placed } = &a.kind else {
        panic!("not a drawing");
    };
    // Twice the size, the top of the box at the top on the page.
    assert_eq!(
        placed[0],
        [PdfPoint::new(200.0, 450.0), PdfPoint::new(400.0, 450.0)]
    );
    assert_eq!(
        placed[1],
        [PdfPoint::new(200.0, 450.0), PdfPoint::new(200.0, 350.0)]
    );
    assert_eq!(
        a.rect,
        PdfBox::new(PdfPoint::new(199.0, 349.0), PdfPoint::new(401.0, 451.0))
    );
    assert!(
        Annotation::drawing(
            PdfPoint::new(0.0, 0.0),
            200.0,
            &[vec![]],
            (100.0, 50.0),
            RED,
            2.0
        )
        .is_none()
    );
}
