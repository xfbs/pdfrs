//! Round trips: write edited documents, read them back with our own parser, render every page and
//! compare it with the original page, and check the file with `qpdf --check` where qpdf is
//! installed.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{
    Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream, dictionary,
};
use specto_doc::{PageRef, Rotation, SourceId};
use specto_render::{Document as _, HayroDocument, RenderRequest};
use specto_source::{OpenError, PdfData, SourceDocument, Target};
use specto_write::{Source, WriteError, replace_file, write, write_unprotected};

const MAIN: SourceId = SourceId(0);
const OTHER: SourceId = SourceId(1);

/// A document of `n` pages, each with a different filled rectangle, in a two-level page tree whose
/// inner node passes down the media box, resources and (for the second half) a rotation of 90
/// degrees.
///
/// Page labels i, ii, then 1, 2 and so on. An outline entry and a link to the last page.
fn fixture(n: usize, color: f64) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let root = doc.new_object_id();
    let left = doc.new_object_id();
    let right = doc.new_object_id();
    let mut page_ids = Vec::new();
    for i in 0..n {
        let content = format!(
            "{color} {} 0.5 rg {} 100 200 300 re f BT /F1 30 Tf 72 700 Td (Page {}) Tj ET",
            i as f64 / n as f64,
            50 + 30 * i,
            i + 1
        );
        let contents = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let parent = if i < n / 2 { left } else { right };
        page_ids.push(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => parent, "Contents" => contents,
        }));
    }
    let last = *page_ids.last().unwrap();
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link",
        "Rect" => vec![72.into(), 690.into(), 300.into(), 730.into()],
        "Dest" => vec![last.into(), "Fit".into()],
    });
    doc.get_dictionary_mut(page_ids[0])
        .unwrap()
        .set("Annots", vec![Object::Reference(link)]);
    let (l, r) = page_ids.split_at(n / 2);
    let refs = |ids: &[lopdf::ObjectId]| {
        ids.iter()
            .map(|&i| Object::Reference(i))
            .collect::<Vec<_>>()
    };
    doc.objects.insert(
        left,
        dictionary! {
            "Type" => "Pages", "Parent" => root, "Kids" => refs(l), "Count" => l.len() as i64,
        }
        .into(),
    );
    doc.objects.insert(
        right,
        dictionary! {
            "Type" => "Pages", "Parent" => root, "Kids" => refs(r), "Count" => r.len() as i64,
            "Rotate" => 90,
        }
        .into(),
    );
    doc.objects.insert(
        root,
        dictionary! {
            "Type" => "Pages", "Kids" => vec![left.into(), right.into()], "Count" => n as i64,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        }
        .into(),
    );
    let outlines = doc.new_object_id();
    let item = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Last page"), "Parent" => outlines,
        "Dest" => vec![last.into(), "Fit".into()],
    });
    doc.objects.insert(
        outlines,
        dictionary! { "Type" => "Outlines", "First" => item, "Last" => item, "Count" => 1 }.into(),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => root, "Outlines" => outlines,
        "PageLabels" => dictionary! {
            "Nums" => vec![0.into(), dictionary! { "S" => "r" }.into(), 2.into(), dictionary! { "S" => "D" }.into()],
        },
    });
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
        permissions: Permissions::all(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn main_source(data: &PdfData) -> Source<'_> {
    Source {
        id: MAIN,
        data,
        password: None,
    }
}

fn page(source: SourceId, page: u32) -> PageRef {
    PageRef::new(source, page)
}

fn rotated(source: SourceId, page: u32, rotation: Rotation) -> PageRef {
    PageRef {
        rotation,
        ..PageRef::new(source, page)
    }
}

fn open(data: &[u8], password: Option<&str>) -> HayroDocument {
    HayroDocument::open(data.to_vec(), password).expect("the written file opens")
}

fn render(doc: &HayroDocument, page: usize, rotation: Rotation) -> Vec<u8> {
    let mut req = RenderRequest::page(page, 0.25);
    req.rotation = rotation;
    doc.render(&req).unwrap().data
}

/// Each page of `written` looks like the page it came from (in `sources`, by source id), turned by
/// its rotation.
fn assert_pages(written: &HayroDocument, expected: &[(&HayroDocument, PageRef)]) {
    assert_eq!(written.page_count(), expected.len());
    for (i, (source, p)) in expected.iter().enumerate() {
        assert!(
            render(written, i, Rotation::None) == render(source, p.page as usize, p.rotation),
            "page {i} differs from page {} of its source",
            p.page
        );
    }
}

/// `qpdf --check` passes, if qpdf is installed.
fn qpdf_check(data: &[u8], password: Option<&str>) {
    let path = std::env::temp_dir().join(format!(
        "specto-write-{}-{:x}.pdf",
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
fn unchanged_pages_look_the_same() {
    let data = PdfData::from(fixture(6, 0.2));
    let pages: Vec<PageRef> = (0..6).map(|i| page(MAIN, i)).collect();
    let out = write(&[main_source(&data)], &pages).unwrap();
    qpdf_check(&out, None);
    let (original, written) = (open(&data, None), open(&out, None));
    let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
    assert_pages(&written, &expected);
    // Labels, the outline and links survive.
    let source = SourceDocument::open(out, None).unwrap();
    let labels = source.page_labels().unwrap();
    assert_eq!(labels.label(1), "ii");
    assert_eq!(labels.label(2), "1");
    assert_eq!(source.outline()[0].title, "Last page");
    assert!(matches!(
        source.outline()[0].target,
        Some(Target::Destination(d)) if d.page == 5
    ));
}

#[test]
fn rotation_adds_to_the_page_and_inherited_rotation() {
    let data = PdfData::from(fixture(4, 0.3));
    // Page 1 has no /Rotate of its own, page 3 inherits 90 degrees.
    let pages = vec![
        rotated(MAIN, 0, Rotation::Cw90),
        page(MAIN, 1),
        rotated(MAIN, 2, Rotation::Cw270),
        rotated(MAIN, 3, Rotation::Cw180),
    ];
    let out = write(&[main_source(&data)], &pages).unwrap();
    qpdf_check(&out, None);
    let (original, written) = (open(&data, None), open(&out, None));
    let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
    assert_pages(&written, &expected);
    let rotation = |i| written.page_geometry(i).unwrap().rotation();
    assert_eq!(rotation(0), Rotation::Cw90);
    assert_eq!(rotation(2), Rotation::None);
    assert_eq!(rotation(3), Rotation::Cw270);
}

#[test]
fn deleting_and_moving_pages() {
    let data = PdfData::from(fixture(6, 0.4));
    let pages = vec![page(MAIN, 4), page(MAIN, 0), page(MAIN, 2)];
    let out = write(&[main_source(&data)], &pages).unwrap();
    qpdf_check(&out, None);
    let (original, written) = (open(&data, None), open(&out, None));
    let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
    assert_pages(&written, &expected);
    let source = SourceDocument::open(out.clone(), None).unwrap();
    // Each page keeps its label.
    let original = SourceDocument::open(data.clone(), None).unwrap();
    let before = original.page_labels().unwrap();
    let after = source.page_labels().unwrap();
    for (i, p) in pages.iter().enumerate() {
        assert_eq!(after.label(i), before.label(p.page as usize));
    }
    // The outline entry stays, but its page is gone.
    assert_eq!(source.outline()[0].title, "Last page");
    assert!(source.outline()[0].target.is_none());
    // The deleted pages' contents aren't in the file.
    assert!(out.len() < data.len());
    let only_kept = write(&[main_source(&data)], &[page(MAIN, 0)]).unwrap();
    assert!(only_kept.len() < out.len());
}

#[test]
fn a_page_used_twice() {
    let data = PdfData::from(fixture(2, 0.5));
    let pages = vec![
        page(MAIN, 0),
        rotated(MAIN, 0, Rotation::Cw90),
        page(MAIN, 1),
    ];
    let out = write(&[main_source(&data)], &pages).unwrap();
    qpdf_check(&out, None);
    let (original, written) = (open(&data, None), open(&out, None));
    let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
    assert_pages(&written, &expected);
    // Both copies have the link.
    let source = SourceDocument::open(out, None).unwrap();
    assert_eq!(source.links(0).len(), 1);
    assert_eq!(source.links(1).len(), 1);
}

#[test]
fn pages_from_another_file() {
    let a = PdfData::from(fixture(3, 0.1));
    let b = PdfData::from(fixture(4, 0.9));
    let sources = [
        main_source(&a),
        Source {
            id: OTHER,
            data: &b,
            password: None,
        },
    ];
    let pages = vec![
        page(MAIN, 0),
        page(OTHER, 3),
        rotated(OTHER, 0, Rotation::Cw90),
        page(MAIN, 2),
    ];
    let out = write(&sources, &pages).unwrap();
    qpdf_check(&out, None);
    let (da, db, written) = (open(&a, None), open(&b, None), open(&out, None));
    let expected: Vec<_> = pages
        .iter()
        .map(|&p| (if p.source == MAIN { &da } else { &db }, p))
        .collect();
    assert_pages(&written, &expected);
    // The main document's outline is kept.
    let source = SourceDocument::open(out, None).unwrap();
    assert_eq!(source.outline().len(), 1);
    assert!(matches!(
        write(&sources[..1], &pages),
        Err(WriteError::UnknownSource)
    ));
    assert!(matches!(
        write(&sources, &[page(OTHER, 9)]),
        Err(WriteError::MissingPage(9))
    ));
}

#[test]
fn encryption_is_kept() {
    let data = PdfData::from(encrypted(&fixture(3, 0.6), "secret"));
    let src = |password| Source {
        id: MAIN,
        data: &data,
        password,
    };
    let pages = vec![rotated(MAIN, 2, Rotation::Cw90), page(MAIN, 0)];
    assert!(matches!(
        write(&[src(None)], &pages),
        Err(WriteError::PasswordRequired)
    ));
    assert!(matches!(
        write(&[src(Some("wrong"))], &pages),
        Err(WriteError::PasswordRequired)
    ));
    for password in ["secret", "owner"] {
        let out = write(&[src(Some(password))], &pages).unwrap();
        qpdf_check(&out, Some("secret"));
        assert!(matches!(
            SourceDocument::open(out.clone(), None),
            Err(OpenError::PasswordRequired)
        ));
        let (original, written) = (open(&data, Some("secret")), open(&out, Some("secret")));
        let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
        assert_pages(&written, &expected);
        // The owner password still works too.
        open(&out, Some("owner"));
    }
}

#[test]
fn encryption_can_be_left_out() {
    let data = PdfData::from(encrypted(&fixture(3, 0.6), "secret"));
    let src = |password| Source {
        id: MAIN,
        data: &data,
        password,
    };
    let pages = vec![rotated(MAIN, 2, Rotation::Cw90), page(MAIN, 0)];
    // Not without the password, though.
    assert!(matches!(
        write_unprotected(&[src(None)], &pages),
        Err(WriteError::PasswordRequired)
    ));
    let out = write_unprotected(&[src(Some("secret"))], &pages).unwrap();
    qpdf_check(&out, None);
    let (original, written) = (open(&data, Some("secret")), open(&out, None));
    let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
    assert_pages(&written, &expected);

    // A document without protection is written as ever.
    let plain = PdfData::from(fixture(3, 0.6));
    let out = write_unprotected(&[main_source(&plain)], &pages).unwrap();
    qpdf_check(&out, None);
    let (original, written) = (open(&plain, None), open(&out, None));
    let expected: Vec<_> = pages.iter().map(|&p| (&original, p)).collect();
    assert_pages(&written, &expected);
}

#[test]
fn replacing_a_file() {
    let dir = std::env::temp_dir().join(format!("specto-replace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.pdf");
    std::fs::write(&path, b"old contents").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    // A mapping of the old file keeps seeing the old contents.
    let old = specto_source::PdfData::map(&std::fs::File::open(&path).unwrap()).unwrap();
    replace_file(&path, b"new contents").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new contents");
    assert_eq!(&*old, b"old contents");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        // Through a symbolic link: the link stays, its target is replaced.
        let link = dir.join("link.pdf");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        replace_file(&link, b"via link").unwrap();
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"via link");
    }
    // A new file, and no temporary files left behind.
    replace_file(&dir.join("new.pdf"), b"fresh").unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Rewrites every corpus file twice, unchanged and with the pages reversed and the first one
/// rotated, and compares page count, page geometry and renders of the first pages with the
/// original.
///
/// Prints the files that differ. `cargo test --release -p specto-write -- --ignored --nocapture`
#[test]
#[ignore = "needs corpus/files (run corpus/fetch.sh)"]
fn corpus_round_trip() {
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
    let (mut files, mut written) = (0, 0);
    let (mut failed, mut differ, mut panics) = (Vec::new(), Vec::new(), Vec::new());
    for p in &paths {
        let data = PdfData::open(p).unwrap();
        let Ok(original) = HayroDocument::open(data.clone(), None) else {
            continue;
        };
        files += 1;
        let n = original.page_count() as u32;
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let r = catch_unwind(AssertUnwindSafe(|| {
            if n == 0 {
                return Ok(None);
            }
            let same: Vec<PageRef> = (0..n).map(|i| page(MAIN, i)).collect();
            let mut changed: Vec<PageRef> = (0..n).rev().map(|i| page(MAIN, i)).collect();
            changed[0].rotation = Rotation::Cw90;
            for pages in [same, changed] {
                let out = match write(&[main_source(&data)], &pages) {
                    Ok(out) => out,
                    Err(e) => return Err(format!("{name}: {e}")),
                };
                let Ok(copy) = HayroDocument::open(out, None) else {
                    return Err(format!("{name}: the result doesn't open"));
                };
                if copy.page_count() != pages.len() {
                    return Ok(Some(format!(
                        "{name}: {} pages instead of {}",
                        copy.page_count(),
                        pages.len()
                    )));
                }
                for (i, p) in pages.iter().enumerate() {
                    let g = original
                        .page_geometry(p.page as usize)
                        .unwrap()
                        .rotated(p.rotation);
                    if copy.page_geometry(i).unwrap() != g {
                        return Ok(Some(format!(
                            "{name}: page {i} has another size or rotation"
                        )));
                    }
                }
                for (i, p) in pages.iter().enumerate().take(2) {
                    let render = |d: &HayroDocument, page, rotation| {
                        let mut req = RenderRequest::page(page, 0.1);
                        req.rotation = rotation;
                        d.render(&req).map(|b| b.data).ok()
                    };
                    if render(&copy, i, Rotation::None)
                        != render(&original, p.page as usize, p.rotation)
                    {
                        return Ok(Some(format!("{name}: page {i} looks different")));
                    }
                }
            }
            Ok(None)
        }));
        match r {
            Ok(Ok(None)) => written += 1,
            Ok(Ok(Some(d))) => {
                written += 1;
                differ.push(d);
            }
            Ok(Err(e)) => failed.push(e),
            Err(_) => panics.push(name),
        }
    }
    println!(
        "{files} files · {written} written · {} differ · {} failed · {} panicked",
        differ.len(),
        failed.len(),
        panics.len()
    );
    for d in differ.iter().chain(&failed).chain(&panics) {
        println!("  {d}");
    }
    assert!(files > 0);
}
