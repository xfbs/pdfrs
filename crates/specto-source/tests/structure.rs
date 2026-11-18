use specto_geom::PdfPoint;
use specto_source::{Date, Destination, NamedAction, OpenError, SourceDocument, Target};

/// Builds a PDF from numbered object bodies (`objects[i]` becomes object `i + 1`), with object 1 as
/// the catalog.
///
/// The xref is left out, and hayro rebuilds it.
fn pdf(objects: &[&str]) -> Vec<u8> {
    pdf_with_trailer(objects, "")
}

/// As [`pdf`], with `extra` entries in the trailer and written as PDF 1.4.
fn pdf_with_trailer(objects: &[&str], extra: &str) -> Vec<u8> {
    let version = if extra.is_empty() { "1.7" } else { "1.4" };
    let mut out = format!("%PDF-{version}\n");
    for (i, body) in objects.iter().enumerate() {
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    out.push_str(&format!(
        "trailer << /Root 1 0 R /Size {} {extra} >>\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

/// Three pages (objects 3 to 5), an outline, named destinations and links.
fn sample() -> Vec<u8> {
    pdf(&[
        // 1: catalog
        "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R
            /Dests << /old [4 0 R /Fit] >>
            /Names << /Dests 12 0 R >> >>",
        // 2: page tree
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 600 800] >>",
        // 3 to 5: pages. Page 1 has links
        "<< /Type /Page /Parent 2 0 R /Annots [10 0 R 11 0 R 14 0 R 15 0 R 16 0 R] >>",
        "<< /Type /Page /Parent 2 0 R >>",
        "<< /Type /Page /Parent 2 0 R >>",
        // 6: outline root
        "<< /Type /Outlines /First 7 0 R /Last 9 0 R >>",
        // 7: "Intro", pointing at page 1 at top 700, with one child
        "<< /Title (Intro) /Parent 6 0 R /Next 9 0 R /First 8 0 R /Last 8 0 R
            /Dest [3 0 R /XYZ 50 700 null] >>",
        // 8: child with a UTF-16 title, named destination via the name tree
        "<< /Title <FEFF0043006800e9> /Parent 7 0 R /Dest (sec2) >>",
        // 9: "Last", GoTo action. Its /Next points back to 7, a cycle
        "<< /Title (Last) /Parent 6 0 R /Next 7 0 R
            /A << /S /GoTo /D [5 0 R /FitH 300] >> >>",
        // 10: link to a URI
        "<< /Type /Annot /Subtype /Link /Rect [10 10 110 30]
            /A << /S /URI /URI (https://example.org/) >> >>",
        // 11: link to a named destination in /Dests (PDF 1.1 style)
        "<< /Type /Annot /Subtype /Link /Rect [200 40 100 20] /Dest /old >>",
        // 12: name tree root with two kids
        "<< /Kids [13 0 R 17 0 R] >>",
        // 13: leaf: (a) to (m)
        "<< /Limits [(a) (m)] /Names [(alpha) [3 0 R /Fit] (beta) [4 0 R /Fit]] >>",
        // 14: Named action link
        "<< /Type /Annot /Subtype /Link /Rect [300 300 400 400]
            /A << /S /Named /N /NextPage >> >>",
        // 15: unsupported action (JavaScript): no link
        "<< /Type /Annot /Subtype /Link /Rect [0 0 1 1]
            /A << /S /JavaScript /JS (app.alert(1)) >> >>",
        // 16: not a link
        "<< /Type /Annot /Subtype /Square /Rect [0 0 50 50] >>",
        // 17: leaf: (n) to (z)
        "<< /Limits [(n) (z)] /Names [(sec2) << /D [4 0 R /XYZ null 500 0] >>] >>",
    ])
}

fn dest(page: usize, left: Option<f64>, top: Option<f64>) -> Option<Target> {
    Some(Target::Destination(Destination { page, left, top }))
}

#[test]
fn outline_with_nesting_names_actions_and_a_cycle() {
    let doc = SourceDocument::open(sample(), None).unwrap();
    assert_eq!(doc.page_count(), 3);
    let outline = doc.outline();
    // The cycle (from Last through /Next back to Intro) stops after "Last".
    let titles: Vec<&str> = outline.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["Intro", "Last"]);
    assert_eq!(outline[0].target, dest(0, Some(50.0), Some(700.0)));
    assert_eq!(outline[0].children.len(), 1);
    let child = &outline[0].children[0];
    assert_eq!(child.title, "Ché");
    // Named destination found in the second leaf of the name tree, as a destination dictionary,
    // with a null left coordinate.
    assert_eq!(child.target, dest(1, None, Some(500.0)));
    assert_eq!(outline[1].target, dest(2, None, Some(300.0)));
}

#[test]
fn links_and_their_targets() {
    let doc = SourceDocument::open(sample(), None).unwrap();
    let links = doc.links(0);
    assert_eq!(links.len(), 3, "{links:?}"); // JavaScript and Square are skipped
    assert_eq!(links[0].target, Target::Uri("https://example.org/".into()));
    // Reversed coordinates are normalized.
    assert_eq!(
        (links[1].rect.min, links[1].rect.max),
        (PdfPoint::new(100.0, 20.0), PdfPoint::new(200.0, 40.0))
    );
    assert_eq!(Some(links[1].target.clone()), dest(1, None, None));
    assert_eq!(links[2].target, Target::Named(NamedAction::NextPage));

    assert_eq!(
        doc.link_at(0, PdfPoint::new(50.0, 20.0)).map(|l| l.target),
        Some(Target::Uri("https://example.org/".into()))
    );
    assert!(doc.link_at(0, PdfPoint::new(500.0, 500.0)).is_none());
    assert!(doc.links(1).is_empty());
    assert!(doc.links(99).is_empty());
}

#[test]
fn page_labels_from_a_number_tree() {
    let doc = SourceDocument::open(
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /PageLabels << /Kids [7 0 R 8 0 R] >> >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 /MediaBox [0 0 100 100] >>",
            "<< /Type /Page /Parent 2 0 R >>",
            "<< /Type /Page /Parent 2 0 R >>",
            "<< /Type /Page /Parent 2 0 R >>",
            "<< /Type /Page /Parent 2 0 R >>",
            // Kids split the ranges. The second kid's dictionary is a reference.
            "<< /Limits [0 0] /Nums [0 << /S /r >>] >>",
            "<< /Limits [2 3] /Nums [2 << /S /D /St 7 >> 3 9 0 R] >>",
            "<< /S /A /P <FEFF00410070> >>",
        ]),
        None,
    )
    .unwrap();
    let labels = doc.page_labels().expect("labels");
    let all: Vec<String> = (0..4).map(|i| labels.label(i)).collect();
    assert_eq!(all, ["i", "ii", "7", "ApA"]);
    assert_eq!(labels.find("ii"), Some(1));
}

#[test]
fn metadata_with_dates_and_the_version() {
    let data = pdf_with_trailer(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>",
            "<< /Type /Page /Parent 2 0 R >>",
            "<< /Title <FEFF0043006800e9> /Author (  Ada  ) /Subject () /Producer (specto tests)
                /CreationDate (D:20240305142210+01'00') /ModDate (D:20250101000000-05'30') >>",
        ],
        "/Info 4 0 R",
    );
    let doc = SourceDocument::open(data, None).unwrap();
    let m = doc.metadata();
    assert_eq!(m.title.as_deref(), Some("Ché"));
    assert_eq!(m.author.as_deref(), Some("Ada"));
    assert_eq!((m.subject, m.keywords, m.creator), (None, None, None));
    assert_eq!(m.producer.as_deref(), Some("specto tests"));
    let created = m.created.unwrap();
    assert_eq!(created.utc_offset_minutes, 60);
    assert_eq!(created.to_string(), "2024-03-05T14:22:10+01:00");
    let modified = m.modified.unwrap();
    assert_eq!(modified.utc_offset_minutes, -330);
    assert_eq!(modified.to_string(), "2025-01-01T00:00:00-05:30");
    assert_eq!(doc.version(), "1.4");
}

#[test]
fn time_zones_as_programs_write_them() {
    let offset = |text: &str| Date::parse(text.as_bytes()).map(|d| d.utc_offset_minutes);
    assert_eq!(offset("D:20240305142210+01'00'"), Some(60));
    assert_eq!(offset("D:20240305142210+01'00"), Some(60));
    assert_eq!(offset("D:20240305142210+0100"), Some(60));
    assert_eq!(offset("D:20240305142210-05:30"), Some(-330));
    assert_eq!(offset("D:20240305142210+01"), Some(60));
    assert_eq!(offset("D:20240305142210Z"), Some(0));
    assert_eq!(offset("D:20240305142210+2500"), None);
}

#[test]
fn documents_without_structure() {
    let doc = SourceDocument::open(
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 100 100] >>",
            "<< /Type /Page /Parent 2 0 R >>",
        ]),
        None,
    )
    .unwrap();
    assert!(doc.outline().is_empty());
    assert!(doc.links(0).is_empty());
    assert!(doc.page_labels().is_none());
    assert_eq!(doc.metadata(), Default::default());
    assert!(matches!(
        SourceDocument::open(b"nope".to_vec(), None),
        Err(OpenError::Invalid)
    ));
}

/// Walks outline and links of every corpus file: no panics, and reports how many files have each.
///
/// `cargo test -p specto-source -- --ignored --nocapture`
#[test]
#[ignore = "needs corpus/files (run corpus/fetch.sh)"]
fn corpus_structure_does_not_panic() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/files");
    let mut stack = vec![dir];
    let (mut files, mut with_outline, mut with_links, mut panics) = (0, 0, 0, Vec::new());
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|e| e != "pdf") {
                continue;
            }
            let Ok(doc) = SourceDocument::open(std::fs::read(&p).unwrap(), None) else {
                continue;
            };
            files += 1;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let o = !doc.outline().is_empty();
                let _ = doc.metadata();
                let l = (0..doc.page_count().min(50)).any(|i| !doc.links(i).is_empty());
                if let Some(labels) = doc.page_labels() {
                    for i in 0..doc.page_count().min(500) {
                        let _ = labels.label(i);
                    }
                }
                (o, l)
            }));
            match r {
                Ok((o, l)) => {
                    with_outline += o as usize;
                    with_links += l as usize;
                }
                Err(_) => panics.push(p.display().to_string()),
            }
        }
    }
    println!("{files} files · {with_outline} with an outline · {with_links} with links");
    assert!(files > 0);
    assert!(panics.is_empty(), "panicked on: {panics:#?}");
}
