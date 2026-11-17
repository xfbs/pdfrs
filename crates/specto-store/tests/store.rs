//! Drafts written, read back, pruned, and read by later versions.

use std::fs::{self, File};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use specto_annot::{
    AnnotKind, Annotation, Color, Date, FontFamily, MarkupStyle, Origin, StampArt, StampImage,
    TextFont,
};
use specto_doc::{AnnotId, EditDoc, EditState, FieldId, FieldValue, PageRef, SourceId};
use specto_geom::{PdfBox, PdfPoint, PdfQuad, Rotation};
use specto_session::Draft;
use specto_store::{Store, StoreError};

const MAIN: SourceId = SourceId(0);
const INSERTED: SourceId = SourceId(1);

/// The key and fingerprint of the draft of the fixtures.
const KEY: &str = "fixture";

fn rect(x: f64, y: f64) -> PdfBox {
    PdfBox::new(PdfPoint::new(x, y), PdfPoint::new(x + 40.0, y + 20.0))
}

fn signature() -> StampImage {
    StampImage {
        width: 3,
        height: 2,
        rgba: (0..24).collect(),
    }
}

/// Edits of every kind there is: pages turned and inserted, each kind of annotation added, one of
/// the file changed and one deleted, and each kind of value filled in.
fn edits() -> EditState {
    let mut doc = EditDoc::from_source(MAIN, 2);
    doc.rotate(&[1], Rotation::Cw270);
    doc.insert(1, vec![PageRef::new(INSERTED, 0)]);
    let quad = PdfQuad::new([
        PdfPoint::new(10.0, 10.0),
        PdfPoint::new(50.0, 10.0),
        PdfPoint::new(50.0, 20.0),
        PdfPoint::new(10.0, 20.0),
    ]);
    let kinds = [
        AnnotKind::TextMarkup {
            style: MarkupStyle::Squiggly,
            quads: vec![quad],
        },
        AnnotKind::Ink {
            strokes: vec![vec![PdfPoint::new(1.0, 2.0), PdfPoint::new(3.0, 4.5)]],
        },
        AnnotKind::Square {
            fill: Some(Color::new(0.0, 0.5, 1.0)),
        },
        AnnotKind::Circle { fill: None },
        AnnotKind::Line {
            from: PdfPoint::new(0.0, 0.0),
            to: PdfPoint::new(100.0, 50.0),
        },
        AnnotKind::Note,
        AnnotKind::FreeText {
            size: 12.0,
            font: TextFont {
                family: FontFamily::Serif,
                bold: true,
                italic: false,
            },
        },
        AnnotKind::Stamp(StampArt::Image(Arc::new(signature()))),
        AnnotKind::Stamp(StampArt::InFile),
    ];
    let added = kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| {
            let mut a = Annotation::new(kind, rect(10.0, 30.0 * i as f64));
            a.color = Some(Color::new(1.0, 0.0, 0.0));
            a.opacity = 0.5;
            a.contents = Some(format!("Annotation {i}"));
            a.author = Some("Ada".into());
            (MAIN, 0, a)
        })
        .collect();
    doc.add_annotations(added);
    let mut stamp = Annotation::new(
        AnnotKind::Stamp(StampArt::Image(Arc::new(signature()))),
        rect(200.0, 200.0),
    );
    stamp.modified = Some(Date {
        year: 2026,
        month: 11,
        day: 17,
        hour: 9,
        minute: 30,
        second: 0,
        utc_offset_minutes: 60,
    });
    doc.add_annotation(INSERTED, 0, stamp);

    let mut changed = Annotation::new(
        AnnotKind::Other {
            subtype: "Polygon".into(),
        },
        rect(5.0, 5.0),
    );
    changed.origin = Some(Origin {
        index: 0,
        object: Some((14, 0)),
    });
    let first = AnnotId::File {
        source: MAIN,
        page: 1,
        index: 0,
    };
    doc.change_annotation(first, changed);
    let mut deleted = Annotation::new(AnnotKind::Note, rect(5.0, 5.0));
    deleted.origin = Some(Origin {
        index: 1,
        object: None,
    });
    let second = AnnotId::File {
        source: MAIN,
        page: 1,
        index: 1,
    };
    doc.remove_annotation(second, &deleted);

    let values = [
        FieldValue::Text("Ada Lovelace".into()),
        FieldValue::Check(true),
        FieldValue::Choice(Some("Yes".into())),
        FieldValue::Several(vec!["Monday".into(), "Friday".into()]),
    ];
    for (i, value) in values.into_iter().enumerate() {
        doc.fill_field(FieldId(20 + i as u32, 0), value, None);
    }
    doc.state()
}

fn draft() -> Draft {
    Draft {
        state: edits(),
        files: vec![b"%PDF-1.7 the inserted file".to_vec().into()],
    }
}

/// The edits as JSON, to compare them.
fn json(state: &EditState) -> serde_json::Value {
    serde_json::to_value(state).unwrap()
}

#[test]
fn a_draft_is_read_back_as_written() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(store.load_draft("a", "1").unwrap().is_none());

    let written = draft();
    store.save_draft("a", "1", &written).unwrap();
    let read = store.load_draft("a", "1").unwrap().unwrap();
    assert_eq!(json(&read.state), json(&written.state));
    assert_eq!(&*read.files[0], &*written.files[0]);
    // The signature is used twice and kept once.
    assert_eq!(fs::read_dir(dir.path().join("images")).unwrap().count(), 1);

    // Another document's draft is another draft.
    assert!(store.load_draft("b", "1").unwrap().is_none());
    // A file that changed has none.
    assert!(matches!(
        store.load_draft("a", "2"),
        Err(StoreError::Changed)
    ));

    store.delete_draft("a").unwrap();
    assert!(store.load_draft("a", "1").unwrap().is_none());
    store.delete_draft("a").unwrap();
}

#[test]
fn a_draft_replaces_the_one_before() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.save_draft("a", "1", &draft()).unwrap();
    let mut doc = EditDoc::from_source(MAIN, 2);
    doc.delete(&[0]);
    let fewer = Draft {
        state: doc.state(),
        files: Vec::new(),
    };
    store.save_draft("a", "1", &fewer).unwrap();
    let read = store.load_draft("a", "1").unwrap().unwrap();
    assert_eq!(read.state.pages().len(), 1);
    assert!(read.files.is_empty());
    let draft_dir = fs::read_dir(dir.path().join("drafts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        fs::read_dir(draft_dir).unwrap().count(),
        1,
        "only the draft"
    );
}

#[test]
fn old_drafts_and_unused_images_are_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.save_draft("old", "1", &draft()).unwrap();
    store.save_draft("new", "1", &draft()).unwrap();
    let month_ago = SystemTime::now() - Duration::from_secs(31 * 24 * 3600);
    let old = fs::read_dir(dir.path().join("drafts"))
        .unwrap()
        .map(|e| e.unwrap().path().join("draft.json"))
        .find(|p| fs::read_to_string(p).unwrap().contains("\"old\""))
        .unwrap();
    File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(month_ago)
        .unwrap();
    // Something left unreadable is pruned whatever its age.
    fs::create_dir(dir.path().join("drafts/broken")).unwrap();

    let thirty_days = Duration::from_secs(30 * 24 * 3600);
    store.prune(thirty_days).unwrap();
    assert!(store.load_draft("old", "1").unwrap().is_none());
    assert!(store.load_draft("new", "1").unwrap().is_some());
    assert!(!dir.path().join("drafts/broken").exists());
    let images = || fs::read_dir(dir.path().join("images")).unwrap().count();
    assert_eq!(images(), 1, "still in use");

    store.delete_draft("new").unwrap();
    store.prune(thirty_days).unwrap();
    assert_eq!(images(), 0);
}

/// A store for each format there was, with the draft of [`edits`] as it was when the format was
/// new.
fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/drafts")
}

#[test]
fn drafts_of_earlier_versions_are_read() {
    let mut formats = 0;
    for entry in fs::read_dir(fixtures()).unwrap() {
        let fixture = entry.unwrap().path();
        // Loading touches the draft, so a copy.
        let dir = tempfile::tempdir().unwrap();
        copy(&fixture, dir.path());
        let store = Store::open(dir.path()).unwrap();
        let draft = store
            .load_draft(KEY, KEY)
            .unwrap_or_else(|e| panic!("{}: {e}", fixture.display()))
            .unwrap_or_else(|| panic!("{}: no draft", fixture.display()));
        // What every format's fixture has, whatever was added to [`edits`] since.
        let doc = EditDoc::restored(draft.state);
        let pages: Vec<(SourceId, u32, Rotation)> = (doc.pages().iter())
            .map(|p| (p.source, p.page, p.rotation))
            .collect();
        assert_eq!(
            pages,
            [
                (MAIN, 0, Rotation::None),
                (INSERTED, 0, Rotation::None),
                (MAIN, 1, Rotation::Cw270)
            ]
        );
        let on_first = doc.annotation_changes(MAIN, 0);
        assert_eq!(on_first.added.len(), 9);
        assert_eq!(on_first.added[3].contents.as_deref(), Some("Annotation 3"));
        let on_second = doc.annotation_changes(MAIN, 1);
        assert_eq!((on_second.replaced.len(), on_second.removed.len()), (1, 1));
        let inserted = doc.annotation_changes(INSERTED, 0);
        let AnnotKind::Stamp(StampArt::Image(image)) = &inserted.added[0].kind else {
            panic!("{}: not the signature", fixture.display());
        };
        assert_eq!(**image, signature());
        assert_eq!(doc.form_values().len(), 4);
        assert_eq!(&*draft.files[0], b"%PDF-1.7 the inserted file");
        formats += 1;
    }
    assert!(formats >= 1);
}

/// Writes the fixture of the current format: run it once for a new format, and commit the result.
///
/// ```sh
/// cargo test -p specto-store --test store -- --ignored write_the_fixture
/// ```
#[test]
#[ignore = "writes the fixture of a new format"]
fn write_the_fixture() {
    let dir = fixtures().join(format!("v{}", specto_store::FORMAT));
    assert!(!dir.exists(), "the fixture of this format exists");
    let store = Store::open(&dir).unwrap();
    store.save_draft(KEY, KEY, &draft()).unwrap();
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}
