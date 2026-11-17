//! Forms through a save: filled in, written, read back with our own reader, and rendered with
//! hayro, which shows a field only through its widget's appearance, as other programs do.

use std::process::Command;

use lopdf::{Document, Object, Stream, dictionary};
use specto_doc::{FieldId, FieldValue, PageRef, SourceId};
use specto_forms::{Field, FieldKind, fields};
use specto_render::{Document as _, HayroDocument, RenderRequest};
use specto_source::{PdfData, SourceDocument};
use specto_write::{AnnotChanges, Source, write_filled};

const MAIN: SourceId = SourceId(0);

/// A page of 600 by 800 with a form: a text field "name" at x 100 to 400, y 700 to 730 (white on
/// a pale blue background, with a border), a multiline one "notes" below it, a checkbox "agree"
/// at x 100 to 120, y 500 to 520 whose "Yes" look is a green square, and a radio group "size" of
/// two buttons ("S" and "L") at y 400 to 420 whose looks are red squares. Below them a menu
/// "country" at x 100 to 300, y 300 to 320 (Germany, France and Other, held as "de", "fr" and
/// "Other"), and a list "color" of six colors at y 200 to 260, four of which fit. At the right, a
/// comb "code" of four cells at x 440 to 560, y 700 to 730, and a field "side" at x 500 to 530,
/// y 400 to 600 that is turned a quarter, to be read from the right.
fn fixture() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let page = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let rect =
        |x0: i64, y0: i64, x1: i64, y1: i64| vec![x0.into(), y0.into(), x1.into(), y1.into()];
    let square = |doc: &mut Document, color: &str| {
        doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => rect(0, 0, 20, 20),
            },
            format!("{color} rg 0 0 20 20 re f").into_bytes(),
        ))
    };
    let empty = |doc: &mut Document| {
        doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect(0, 0, 20, 20) },
            Vec::new(),
        ))
    };
    let name = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "P" => page,
        "T" => Object::string_literal("name"), "Rect" => rect(100, 700, 400, 730),
        "MK" => dictionary! {
            "BG" => vec![Object::Real(0.8), Object::Real(0.9), 1.into()],
            "BC" => vec![0.into(), 0.into(), 0.into()],
        },
    });
    let notes = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "P" => page,
        "T" => Object::string_literal("notes"), "Rect" => rect(100, 560, 300, 660),
        "Ff" => 1 << 12, "DA" => Object::string_literal("/Helv 10 Tf 0 0 1 rg"), "Q" => 2,
    });
    let (yes, off) = (square(&mut doc, "0 1 0"), empty(&mut doc));
    let agree = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "P" => page,
        "T" => Object::string_literal("agree"), "Rect" => rect(100, 500, 120, 520),
        "V" => "Off", "AS" => "Off",
        "AP" => dictionary! { "N" => dictionary! { "Yes" => yes, "Off" => off } },
    });
    let size = doc.new_object_id();
    let mut buttons = Vec::new();
    for (i, state) in ["S", "L"].into_iter().enumerate() {
        let (on, off) = (square(&mut doc, "1 0 0"), empty(&mut doc));
        let mut looks = lopdf::Dictionary::new();
        looks.set(state, on);
        looks.set("Off", off);
        buttons.push(doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "P" => page, "Parent" => size,
            "Rect" => rect(100 + 50 * i as i64, 400, 120 + 50 * i as i64, 420),
            "AS" => if state == "S" { "S" } else { "Off" },
            "AP" => dictionary! { "N" => looks },
        }));
    }
    doc.objects.insert(
        size,
        dictionary! {
            "FT" => "Btn", "Ff" => 1 << 15, "T" => Object::string_literal("size"), "V" => "S",
            "Kids" => buttons.iter().map(|&b| Object::Reference(b)).collect::<Vec<_>>(),
        }
        .into(),
    );
    let pair = |value: &str, label: &str| {
        Object::Array(vec![
            Object::string_literal(value),
            Object::string_literal(label),
        ])
    };
    let country = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Ch", "P" => page,
        "T" => Object::string_literal("country"), "Rect" => rect(100, 300, 300, 320),
        "Ff" => 1 << 17, "V" => Object::string_literal("de"), "I" => vec![0.into()],
        "Opt" => vec![pair("de", "Germany"), pair("fr", "France"), Object::string_literal("Other")],
    });
    let colors = ["Red", "Green", "Blue", "Cyan", "Magenta", "Yellow"];
    let color = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Ch", "P" => page,
        "T" => Object::string_literal("color"), "Rect" => rect(100, 200, 300, 260),
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        "Opt" => colors.map(Object::string_literal).to_vec(),
    });
    let code = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "P" => page,
        "T" => Object::string_literal("code"), "Rect" => rect(440, 700, 560, 730),
        "Ff" => 1 << 24, "MaxLen" => 4,
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
    });
    let side = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "P" => page,
        "T" => Object::string_literal("side"), "Rect" => rect(500, 400, 530, 600),
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        "MK" => dictionary! { "R" => 90 },
    });
    let mut annots = vec![name.into(), notes.into(), agree.into()];
    annots.extend(buttons.iter().map(|&b| Object::Reference(b)));
    annots.extend([country, color, code, side].map(Object::Reference));
    doc.objects.insert(
        page,
        dictionary! { "Type" => "Page", "Parent" => pages, "Annots" => annots }.into(),
    );
    doc.objects.insert(
        pages,
        dictionary! {
            "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()],
            "MediaBox" => rect(0, 0, 600, 800), "Resources" => dictionary! {},
        }
        .into(),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => pages,
        "AcroForm" => dictionary! {
            "Fields" => vec![
                name.into(), notes.into(), agree.into(), size.into(), country.into(),
                color.into(), code.into(), side.into(),
            ],
            "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
        },
    });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn read(data: &[u8]) -> Vec<Field> {
    let doc = SourceDocument::open(data.to_vec(), None).expect("the file opens");
    fields(&doc)
}

fn field<'a>(all: &'a [Field], name: &str) -> &'a Field {
    all.iter()
        .find(|f| f.name == name)
        .expect("the field is there")
}

fn fill(data: &[u8], values: &[(FieldId, FieldValue)]) -> Vec<u8> {
    let data = PdfData::from(data.to_vec());
    let source = Source {
        id: MAIN,
        data: &data,
        password: None,
    };
    let pages = [PageRef::new(MAIN, 0)];
    let out =
        write_filled(&[source], &pages, &AnnotChanges::new(), values, true).expect("it saves");
    qpdf_check(&out);
    out
}

/// The page rendered at one pixel a point.
fn page(data: &[u8]) -> specto_render::Bitmap {
    let doc = HayroDocument::open(data.to_vec(), None).expect("the file opens");
    doc.render(&RenderRequest::page(0, 1.0)).unwrap()
}

fn pixel(page: &specto_render::Bitmap, x: u32, y: u32) -> [u8; 3] {
    let at = ((799 - y) * page.width + x) as usize * 4;
    [page.data[at], page.data[at + 1], page.data[at + 2]]
}

/// How many pixels between two corners (PDF points) `is` holds for.
fn count(
    page: &specto_render::Bitmap,
    x: (u32, u32),
    y: (u32, u32),
    is: fn([u8; 3]) -> bool,
) -> usize {
    (x.0..x.1)
        .flat_map(|px| (y.0..y.1).map(move |py| (px, py)))
        .filter(|&(px, py)| is(pixel(page, px, py)))
        .count()
}

fn qpdf_check(data: &[u8]) {
    let path = std::env::temp_dir().join(format!(
        "specto-forms-{}-{:x}.pdf",
        std::process::id(),
        data.len() ^ data.iter().take(4096).map(|&b| b as usize).sum::<usize>()
    ));
    std::fs::write(&path, data).unwrap();
    let result = Command::new("qpdf").arg("--check").arg(&path).output();
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
fn the_fixture_is_read() {
    let all = read(&fixture());
    let names: Vec<&str> = all.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "name", "notes", "agree", "size", "country", "color", "code", "side"
        ]
    );
    // A menu with values that differ from what it shows, and a list.
    let country = field(&all, "country");
    let FieldKind::Choice {
        options,
        list: false,
        editable: false,
        multiple: false,
    } = &country.kind
    else {
        panic!("a menu: {:?}", country.kind);
    };
    let shown: Vec<(&str, &str)> = options
        .iter()
        .map(|o| (o.value.as_str(), o.label.as_str()))
        .collect();
    assert_eq!(
        shown,
        [("de", "Germany"), ("fr", "France"), ("Other", "Other")]
    );
    assert_eq!(country.value, Some(FieldValue::Text("de".to_owned())));
    assert!(matches!(
        &field(&all, "color").kind,
        FieldKind::Choice { options, list: true, .. } if options.len() == 6
    ));
    assert_eq!(
        field(&all, "color").value,
        Some(FieldValue::Text(String::new()))
    );
    assert!(matches!(
        field(&all, "name").kind,
        FieldKind::Text {
            multiline: false,
            ..
        }
    ));
    assert!(matches!(
        field(&all, "notes").kind,
        FieldKind::Text {
            multiline: true,
            ..
        }
    ));
    assert!(matches!(
        field(&all, "code").kind,
        FieldKind::Text {
            comb: true,
            max_length: Some(4),
            ..
        }
    ));
    assert!(matches!(
        field(&all, "name").kind,
        FieldKind::Text { comb: false, .. }
    ));
    assert_eq!(field(&all, "notes").look.size, 10.0);
    assert_eq!(field(&all, "notes").look.alignment, 2);
    assert_eq!(
        field(&all, "name").look.size,
        0.0,
        "from the form: as large as fits"
    );
    assert_eq!(field(&all, "agree").kind, FieldKind::Checkbox);
    assert_eq!(field(&all, "agree").value, Some(FieldValue::Check(false)));
    assert_eq!(
        field(&all, "agree").widgets[0].on_state.as_deref(),
        Some("Yes")
    );
    let size = field(&all, "size");
    assert_eq!(size.kind, FieldKind::Radio);
    assert_eq!(size.value, Some(FieldValue::Choice(Some("S".to_owned()))));
    let states: Vec<_> = size.widgets.iter().map(|w| w.on_state.as_deref()).collect();
    assert_eq!(states, [Some("S"), Some("L")]);
    assert!(
        all.iter()
            .all(|f| f.is_fillable() && f.widgets.iter().all(|w| w.page == 0))
    );
}

#[test]
fn filled_in_values_are_saved_and_show() {
    let original = fixture();
    let all = read(&original);
    let id = |name: &str| field(&all, name).id;
    let before = page(&original);
    let dark = |c: [u8; 3]| c.iter().all(|&v| v < 90);
    let blue = |c: [u8; 3]| c[2] > 150 && c[0] < 100 && c[1] < 100;
    let green = |c: [u8; 3]| c[1] > 200 && c[0] < 60 && c[2] < 60;
    let red = |c: [u8; 3]| c[0] > 200 && c[1] < 60 && c[2] < 60;
    assert_eq!(count(&before, (102, 398), (702, 728), dark), 0);
    assert!(red(pixel(&before, 110, 410)) && !red(pixel(&before, 160, 410)));

    let out = fill(
        &original,
        &[
            (
                id("name"),
                FieldValue::Text("Ada Lovelace (née Byron)".to_owned()),
            ),
            (
                id("notes"),
                FieldValue::Text(
                    "A line of notes that is surely long enough to wrap around\nand a second"
                        .to_owned(),
                ),
            ),
            (id("agree"), FieldValue::Check(true)),
            (id("size"), FieldValue::Choice(Some("L".to_owned()))),
        ],
    );

    // Read back: what was filled in.
    let now = read(&out);
    assert_eq!(
        field(&now, "name").value,
        Some(FieldValue::Text("Ada Lovelace (née Byron)".to_owned()))
    );
    assert_eq!(field(&now, "agree").value, Some(FieldValue::Check(true)));
    assert_eq!(
        field(&now, "size").value,
        Some(FieldValue::Choice(Some("L".to_owned())))
    );

    // Shown: the name in its field, over the background and inside the border it says it has.
    let after = page(&out);
    assert!(
        count(&after, (102, 398), (702, 728), dark) > 150,
        "no text in the name field"
    );
    assert_eq!(
        pixel(&after, 380, 715),
        [204, 230, 255],
        "the field's background"
    );
    assert!(dark(pixel(&after, 250, 729)), "the field's border");
    assert_eq!(
        count(&after, (402, 600), (690, 740), dark),
        0,
        "text beyond the field"
    );
    // The notes: blue, in several lines from the top, against the right edge.
    assert!(
        count(&after, (200, 298), (646, 658), blue) > 20,
        "no first line at the right"
    );
    assert!(
        count(&after, (200, 298), (622, 634), blue) > 10,
        "no third line"
    );
    // (The short last line shows it: nothing of it at the left.)
    assert_eq!(
        count(&after, (102, 200), (622, 634), blue),
        0,
        "the notes aren't right-aligned"
    );
    assert_eq!(
        count(&after, (100, 300), (540, 558), blue),
        0,
        "notes below their field"
    );
    // The box ticked, and the other radio button chosen.
    assert!(green(pixel(&after, 110, 510)));
    assert!(!red(pixel(&after, 110, 410)) && red(pixel(&after, 160, 410)));

    // A menu shows what was chosen, by what it is called, and a list marks it, scrolled so far
    // that it shows: the last of six is in the fourth row.
    let marked = |c: [u8; 3]| c[0].abs_diff(153) < 6 && c[1].abs_diff(191) < 6 && c[2] > 205;
    assert_eq!(count(&after, (102, 298), (302, 318), dark), 0);
    let chosen = fill(
        &original,
        &[
            (id("country"), FieldValue::Text("fr".to_owned())),
            (id("color"), FieldValue::Text("Yellow".to_owned())),
        ],
    );
    let now = read(&chosen);
    assert_eq!(
        field(&now, "country").value,
        Some(FieldValue::Text("fr".to_owned()))
    );
    assert_eq!(
        field(&now, "color").value,
        Some(FieldValue::Text("Yellow".to_owned()))
    );
    let shown = page(&chosen);
    let in_menu = count(&shown, (102, 298), (302, 318), dark);
    assert!(in_menu > 40, "nothing in the menu: {in_menu}");
    assert_eq!(
        count(&shown, (150, 298), (302, 318), dark),
        0,
        "more than \"France\" in the menu"
    );
    assert!(
        marked(pixel(&shown, 290, 210)),
        "{:?}",
        pixel(&shown, 290, 210)
    );
    assert!(!marked(pixel(&shown, 290, 225)) && !marked(pixel(&shown, 290, 250)));
    for row in 0..4 {
        let y = 204 + 14 * row;
        assert!(
            count(&shown, (102, 200), (y, y + 10), dark) > 10,
            "row {row} is empty"
        );
    }

    // A comb has one letter in the middle of each cell: four cells of 30 points, and an "H" of
    // 12 points is under 9 wide, so the outer thirds of each cell stay empty. A fifth letter is
    // left out.
    let inked = |c: [u8; 3]| c.iter().all(|&v| v < 200);
    let combed = page(&fill(
        &original,
        &[
            (id("code"), FieldValue::Text("HHHHH".to_owned())),
            (id("side"), FieldValue::Text("Sideways".to_owned())),
        ],
    ));
    for cell in 0..4 {
        let x = 440 + 30 * cell;
        assert!(
            count(&combed, (x + 9, x + 21), (702, 728), inked) > 5,
            "cell {cell} is empty"
        );
        assert_eq!(
            count(&combed, (x, x + 8), (702, 728), inked),
            0,
            "cell {cell} has something at its left"
        );
        assert_eq!(count(&combed, (x + 22, x + 30), (702, 728), inked), 0);
    }
    // The turned field has its text along its length: from the bottom up, in a narrow column in
    // the middle, and as long as a word, not as short as a line is high.
    let rows: Vec<u32> = (400..600)
        .filter(|&y| count(&combed, (500, 530), (y, y + 1), inked) > 0)
        .collect();
    let (low, high) = (rows[0], *rows.last().unwrap());
    assert!(
        low < 410 && (440..470).contains(&high),
        "from {low} to {high}"
    );
    assert_eq!(count(&combed, (500, 506), (400, 600), inked), 0);
    assert_eq!(count(&combed, (524, 530), (400, 600), inked), 0);

    // Emptied and unticked again.
    let cleared = fill(
        &out,
        &[
            (id("name"), FieldValue::Text(String::new())),
            (id("agree"), FieldValue::Check(false)),
            (id("size"), FieldValue::Choice(None)),
        ],
    );
    let after = page(&cleared);
    assert_eq!(count(&after, (102, 398), (702, 728), dark), 0);
    assert_eq!(pixel(&after, 380, 715), [204, 230, 255]);
    assert!(!green(pixel(&after, 110, 510)));
    assert!(!red(pixel(&after, 110, 410)) && !red(pixel(&after, 160, 410)));
    let now = read(&cleared);
    assert_eq!(field(&now, "size").value, Some(FieldValue::Choice(None)));
    // A field that isn't in the file is passed over.
    fill(&original, &[(FieldId(9999, 0), FieldValue::Check(true))]);
}

#[test]
fn values_for_empty_fields_are_appended_to_the_file() {
    let original = fixture();
    let all = read(&original);
    let id = |name: &str| field(&all, name).id;
    let empty = Some(FieldValue::Text(String::new()));
    assert!([None, empty].contains(&field(&all, "name").value));
    let data = PdfData::from(original.clone());
    let source = Source {
        id: MAIN,
        data: &data,
        password: None,
    };
    let pages = [PageRef::new(MAIN, 0)];
    let none = AnnotChanges::new();

    let values = [
        (id("name"), FieldValue::Text("Ada Lovelace".to_owned())),
        (id("agree"), FieldValue::Check(true)),
    ];
    let out = specto_write::append(&source, &pages, &none, &values)
        .expect("no error")
        .expect("the fields were empty");
    qpdf_check(&out);
    assert!(out.starts_with(&original));
    let now = read(&out);
    assert_eq!(
        field(&now, "name").value,
        Some(FieldValue::Text("Ada Lovelace".to_owned()))
    );
    assert_eq!(field(&now, "agree").value, Some(FieldValue::Check(true)));
    let dark = |c: [u8; 3]| c.iter().all(|&v| v < 90);
    assert!(count(&page(&out), (102, 398), (702, 728), dark) > 100);

    // A value that replaces one: the old one must not stay in the file.
    let data = PdfData::from(out.clone());
    let source = Source {
        id: MAIN,
        data: &data,
        password: None,
    };
    let values = [(id("name"), FieldValue::Text("Grace Hopper".to_owned()))];
    let appended = specto_write::append(&source, &pages, &none, &values).expect("no error");
    assert!(appended.is_none());
    let again = specto_write::write_saved(&[source], &pages, &none, &values).expect("it saves");
    assert!(!again.starts_with(&out));
    let old = b"Ada Lovelace";
    assert!(out.windows(old.len()).any(|w| w == old));
    assert!(!again.windows(old.len()).any(|w| w == old));
    assert_eq!(
        field(&read(&again), "name").value,
        Some(FieldValue::Text("Grace Hopper".to_owned()))
    );
}

#[test]
fn several_choices_of_a_list_are_saved_and_show() {
    // The list of colors, taking several: the first four of six show.
    let mut doc = Document::load_mem(&fixture()).unwrap();
    for object in doc.objects.values_mut() {
        if let Ok(dict) = object.as_dict_mut()
            && dict.get(b"T").and_then(Object::as_str).ok() == Some(b"color")
        {
            dict.set("Ff", 1 << 21);
        }
    }
    let mut original = Vec::new();
    doc.save_to(&mut original).unwrap();
    let all = read(&original);
    let color = field(&all, "color");
    assert!(color.is_fillable());
    assert_eq!(color.value, Some(FieldValue::Several(Vec::new())));

    let several =
        |values: &[&str]| FieldValue::Several(values.iter().map(|v| (*v).to_owned()).collect());
    let chosen = fill(&original, &[(color.id, several(&["Green", "Cyan"]))]);
    assert_eq!(
        field(&read(&chosen), "color").value,
        Some(several(&["Green", "Cyan"]))
    );
    // The second and the fourth row are marked.
    let marked = |c: [u8; 3]| c[0].abs_diff(153) < 6 && c[1].abs_diff(191) < 6 && c[2] > 205;
    let shown = page(&chosen);
    let rows = [250, 237, 223, 209].map(|y| marked(pixel(&shown, 290, y)));
    assert_eq!(rows, [false, true, false, true]);

    // One is held as a text, and none as nothing.
    let one = fill(&chosen, &[(color.id, several(&["Blue"]))]);
    assert_eq!(field(&read(&one), "color").value, Some(several(&["Blue"])));
    let shown = page(&one);
    let rows = [250, 237, 223, 209].map(|y| marked(pixel(&shown, 290, y)));
    assert_eq!(rows, [false, false, true, false]);
    let none = fill(&one, &[(color.id, several(&[]))]);
    assert_eq!(field(&read(&none), "color").value, Some(several(&[])));
    let shown = page(&none);
    assert_eq!(count(&shown, (102, 298), (202, 258), marked), 0);
}
