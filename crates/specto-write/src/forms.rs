//! Writing what was filled into a form.
//!
//! A field's value (`/V`) is not what readers show: they show each widget's appearance. So a
//! checkbox or radio button gets the state it is in now (`/AS`, one of the looks the file has for
//! it), and each widget of a text field gets a new appearance stream with the text in it, set as
//! the field asks (its font, size, color and alignment), over the background and border the
//! widget says it has. A choice field gets the same with what was chosen: a menu shows it alone, a
//! list shows it marked among the others.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use specto_annot::{Color, FontFamily, TextFont, encode_text, text_width, wrap};
use specto_doc::{FieldId, FieldValue};

use crate::{WriteError, text_string};

/// How far a field or its widget is looked for up the tree, for what it takes from above.
const MAX_DEPTH: usize = 16;

/// Applies `values` to the form of `doc`, as loaded: object numbers are still the file's. A field
/// that isn't there (any more) is left out.
pub(crate) fn apply(
    doc: &mut Document,
    values: &[(FieldId, FieldValue)],
) -> Result<(), WriteError> {
    for (id, value) in values {
        let field = (id.0, id.1);
        if doc.get_dictionary(field).is_err() {
            continue;
        }
        let widgets = widgets_of(doc, field);
        match value {
            FieldValue::Check(on) => {
                let mut first = None;
                for &w in &widgets {
                    let state = on_state(doc, w);
                    first = first.or(state.clone());
                    let now = state.filter(|_| *on).unwrap_or_else(|| b"Off".to_vec());
                    set(doc, w, "AS", Object::Name(now));
                }
                let name = first.filter(|_| *on).unwrap_or_else(|| b"Off".to_vec());
                set(doc, field, "V", Object::Name(name));
            }
            FieldValue::Choice(chosen) => {
                let chosen = chosen.as_ref().map(|c| c.as_bytes().to_vec());
                for &w in &widgets {
                    let state = on_state(doc, w);
                    let now = state
                        .filter(|s| Some(s) == chosen.as_ref())
                        .unwrap_or_else(|| b"Off".to_vec());
                    set(doc, w, "AS", Object::Name(now));
                }
                let name = chosen.unwrap_or_else(|| b"Off".to_vec());
                set(doc, field, "V", Object::Name(name));
            }
            FieldValue::Several(values) => {
                let Some(choice) = Choice::of(doc, field, values) else {
                    continue;
                };
                // One is a text, more are an array of them, and none is no value.
                let held = match values.as_slice() {
                    [] => None,
                    [one] => Some(text_string(one)),
                    all => Some(Object::Array(all.iter().map(|v| text_string(v)).collect())),
                };
                let numbers = choice.chosen.iter().map(|&i| Object::Integer(i as i64));
                let numbers = Object::Array(numbers.collect());
                if let Ok(dict) = doc.get_dictionary_mut(field) {
                    match held {
                        Some(held) => dict.set("V", held),
                        None => {
                            dict.remove(b"V");
                        }
                    }
                    dict.set("I", numbers);
                }
                let mut look = TextLook::of(doc, field);
                look.multiline = false;
                look.password = false;
                let shown: Vec<&str> = (choice.chosen.iter())
                    .map(|&i| choice.labels[i].as_str())
                    .collect();
                let shown = shown.join(", ");
                let body = if choice.list {
                    Body::List {
                        labels: &choice.labels,
                        chosen: &choice.chosen,
                    }
                } else {
                    Body::Text(&shown)
                };
                for &w in &widgets {
                    let Some(stream) = text_appearance(doc, w, &body, &look) else {
                        continue;
                    };
                    let id = doc.add_object(stream);
                    set(doc, w, "AP", dictionary! { "N" => id }.into());
                }
            }
            FieldValue::Text(text) => {
                set(doc, field, "V", text_string(text));
                let mut look = TextLook::of(doc, field);
                let choice = Choice::of(doc, field, std::slice::from_ref(text));
                if choice.is_some() {
                    // The numbers of what is chosen, which no longer hold. Readers go by `/V`.
                    if let Ok(dict) = doc.get_dictionary_mut(field) {
                        dict.remove(b"I");
                    }
                    look.multiline = false;
                    look.password = false;
                }
                let body = match &choice {
                    Some(Choice {
                        list: true,
                        labels,
                        chosen,
                    }) => Body::List { labels, chosen },
                    Some(Choice { labels, chosen, .. }) => Body::Text(
                        chosen
                            .first()
                            .map_or(text.as_str(), |&i| labels[i].as_str()),
                    ),
                    None => Body::Text(text),
                };
                for &w in &widgets {
                    let Some(stream) = text_appearance(doc, w, &body, &look) else {
                        continue;
                    };
                    let id = doc.add_object(stream);
                    set(doc, w, "AP", dictionary! { "N" => id }.into());
                }
            }
        }
    }
    Ok(())
}

/// Whether the field `id` of `doc` holds something: a text, a choice, or a box that is on.
pub(crate) fn has_value(doc: &Document, id: FieldId) -> bool {
    match inherited(doc, (id.0, id.1), b"V")
        .as_ref()
        .map(|v| resolve(doc, v))
    {
        None | Some(Object::Null) => false,
        Some(Object::String(text, _)) => !text.is_empty(),
        Some(Object::Name(name)) => name != b"Off",
        Some(Object::Array(values)) => !values.is_empty(),
        Some(_) => true,
    }
}

fn set(doc: &mut Document, id: ObjectId, key: &str, value: Object) {
    if let Ok(dict) = doc.get_dictionary_mut(id) {
        dict.set(key, value);
    }
}

/// The widgets of a field: its kids that aren't fields themselves, or the field if it has none.
fn widgets_of(doc: &Document, field: ObjectId) -> Vec<ObjectId> {
    let kids: Vec<ObjectId> = doc
        .get_dictionary(field)
        .and_then(|d| d.get(b"Kids"))
        .and_then(|k| resolve(doc, k).as_array())
        .map(|kids| kids.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let widgets: Vec<ObjectId> = kids
        .into_iter()
        .filter(|k| doc.get_dictionary(*k).is_ok_and(|d| !d.has(b"T")))
        .collect();
    if widgets.is_empty() {
        vec![field]
    } else {
        widgets
    }
}

/// `object`, or what it refers to.
fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    }
}

/// What a checkbox or radio button's "on" state is called: the look it has other than `Off`.
fn on_state(doc: &Document, widget: ObjectId) -> Option<Vec<u8>> {
    let dict = doc.get_dictionary(widget).ok()?;
    let ap = resolve(doc, dict.get(b"AP").ok()?).as_dict().ok()?;
    let states = resolve(doc, ap.get(b"N").ok()?).as_dict().ok()?;
    states
        .iter()
        .map(|(name, _)| name.clone())
        .find(|name| name != b"Off")
}

/// The value of `key` for `id`, from its own dictionary or the nearest one above it in the field
/// tree, and last from the form itself.
fn inherited(doc: &Document, id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut at = id;
    for _ in 0..MAX_DEPTH {
        let dict = doc.get_dictionary(at).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(resolve(doc, value).clone());
        }
        match dict.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => at = parent,
            Err(_) => break,
        }
    }
    acro_form(doc)?
        .get(key)
        .ok()
        .map(|v| resolve(doc, v).clone())
}

fn acro_form(doc: &Document) -> Option<&Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let form = doc.get_dictionary(root).ok()?.get(b"AcroForm").ok()?;
    resolve(doc, form).as_dict().ok()
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(f64::from(*r)),
        _ => None,
    }
}

fn numbers(doc: &Document, object: &Object) -> Vec<f64> {
    resolve(doc, object)
        .as_array()
        .map(|a| a.iter().filter_map(|n| number(resolve(doc, n))).collect())
        .unwrap_or_default()
}

/// A color as widgets give them: gray, RGB or CMYK by the number of parts.
fn color(parts: &[f64]) -> Option<Color> {
    let p: Vec<f32> = parts.iter().map(|v| (*v as f32).clamp(0.0, 1.0)).collect();
    match p[..] {
        [g] => Some(Color::new(g, g, g)),
        [r, g, b] => Some(Color::new(r, g, b)),
        [c, m, y, k] => Some(Color::new(
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        )),
        _ => None,
    }
}

/// How a text field sets its text.
struct TextLook {
    /// The font's name among the form's resources, and the font.
    font_name: String,
    font: Option<Object>,
    /// The font to measure text with: the nearest of those whose widths are known.
    measure: TextFont,
    /// In points, or 0 for "as large as fits".
    size: f64,
    color: Color,
    /// 0 for left, 1 for centered, 2 for right.
    alignment: i64,
    multiline: bool,
    password: bool,
    /// For a comb field: how many cells of the same width its letters go into, one each.
    comb: Option<usize>,
}

impl TextLook {
    fn of(doc: &Document, field: ObjectId) -> Self {
        let da = inherited(doc, field, b"DA")
            .and_then(|d| {
                d.as_str()
                    .ok()
                    .map(|s| String::from_utf8_lossy(s).into_owned())
            })
            .unwrap_or_default();
        let words: Vec<&str> = da.split_ascii_whitespace().collect();
        let mut font_name = String::new();
        let mut size = 0.0;
        let mut text_color = Color::new(0.0, 0.0, 0.0);
        for (at, word) in words.iter().enumerate() {
            let before = |count: usize| -> Vec<f64> {
                at.checked_sub(count)
                    .map(|from| {
                        words[from..at]
                            .iter()
                            .filter_map(|w| w.parse().ok())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            match *word {
                "Tf" if at >= 2 => {
                    font_name = words[at - 2].trim_start_matches('/').to_owned();
                    size = words[at - 1].parse().unwrap_or(0.0);
                }
                "g" => text_color = color(&before(1)).unwrap_or(text_color),
                "rg" => text_color = color(&before(3)).unwrap_or(text_color),
                "k" => text_color = color(&before(4)).unwrap_or(text_color),
                _ => {}
            }
        }
        // The font, from the form's resources.
        let font = acro_form(doc)
            .and_then(|form| form.get(b"DR").ok())
            .and_then(|dr| resolve(doc, dr).as_dict().ok())
            .and_then(|dr| dr.get(b"Font").ok())
            .and_then(|fonts| resolve(doc, fonts).as_dict().ok())
            .and_then(|fonts| fonts.get(font_name.as_bytes()).ok())
            .cloned();
        let base = font
            .as_ref()
            .and_then(|f| resolve(doc, f).as_dict().ok())
            .and_then(|f| f.get(b"BaseFont").ok())
            .and_then(|n| n.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_lowercase())
            .unwrap_or_else(|| font_name.to_lowercase());
        let measure = TextFont {
            family: if base.contains("times") || base.starts_with("ti") {
                FontFamily::Serif
            } else if base.contains("cour") || base.contains("mono") {
                FontFamily::Mono
            } else {
                FontFamily::Sans
            },
            bold: base.contains("bold") || base.ends_with("bo"),
            italic: base.contains("italic") || base.contains("oblique"),
        };
        let flags = inherited(doc, field, b"Ff")
            .and_then(|f| f.as_i64().ok())
            .unwrap_or(0);
        Self {
            font_name,
            font,
            measure,
            size: if (0.0..=1000.0).contains(&size) {
                size
            } else {
                0.0
            },
            color: text_color,
            alignment: inherited(doc, field, b"Q")
                .and_then(|q| q.as_i64().ok())
                .unwrap_or(0),
            multiline: flags & (1 << 12) != 0,
            password: flags & (1 << 13) != 0,
            // Only with a length, and not for several lines or passwords.
            comb: inherited(doc, field, b"MaxLen")
                .and_then(|m| m.as_i64().ok())
                .and_then(|m| usize::try_from(m).ok())
                .filter(|&m| (1..=1000).contains(&m))
                .filter(|_| flags & (1 << 24) != 0 && flags & ((1 << 12) | (1 << 13)) == 0),
        }
    }
}

/// What a choice field offers, and what of it is chosen.
struct Choice {
    /// A list that shows several options at once, not a menu.
    list: bool,
    /// What the user sees for each option.
    labels: Vec<String>,
    /// Which options are chosen: those the values name, in the order of the list.
    chosen: Vec<usize>,
}

impl Choice {
    /// The choice field `field` holding `values`, or `None` if it is a field of another kind.
    fn of(doc: &Document, field: ObjectId, values: &[String]) -> Option<Self> {
        let kind = inherited(doc, field, b"FT")?;
        if kind.as_name().ok()? != b"Ch" {
            return None;
        }
        let flags = inherited(doc, field, b"Ff")
            .and_then(|f| f.as_i64().ok())
            .unwrap_or(0);
        let string = |o: &Object| match resolve(doc, o) {
            Object::String(bytes, _) => Some(specto_source::decode_text(bytes)),
            _ => None,
        };
        let options: Vec<(String, String)> = inherited(doc, field, b"Opt")
            .as_ref()
            .and_then(|opt| opt.as_array().ok())
            .map(|opt| {
                opt.iter()
                    .filter_map(|entry| match resolve(doc, entry) {
                        Object::Array(pair) => {
                            let value = string(pair.first()?)?;
                            let label = pair.get(1).and_then(string);
                            Some((value.clone(), label.unwrap_or(value)))
                        }
                        other => string(other).map(|v| (v.clone(), v)),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            list: flags & (1 << 17) == 0,
            chosen: (0..options.len())
                .filter(|&i| values.contains(&options[i].0))
                .collect(),
            labels: options.into_iter().map(|(_, label)| label).collect(),
        })
    }
}

/// What a widget's appearance shows.
enum Body<'a> {
    /// Text, as a text field or a menu has it.
    Text(&'a str),
    /// The options of a list, with the chosen ones marked.
    List {
        labels: &'a [String],
        chosen: &'a [usize],
    },
}

/// The appearance of a text or choice field's widget showing `body`.
fn text_appearance(
    doc: &Document,
    widget: ObjectId,
    body: &Body<'_>,
    look: &TextLook,
) -> Option<Stream> {
    let dict = doc.get_dictionary(widget).ok()?;
    let rect = numbers(doc, dict.get(b"Rect").ok()?);
    let [x0, y0, x1, y1] = rect[..] else {
        return None;
    };
    let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
    // What the widget says of its background and border.
    let mk = dict
        .get(b"MK")
        .ok()
        .and_then(|mk| resolve(doc, mk).as_dict().ok());
    // A widget on a page that is shown turned says by how much it is turned itself, so that it
    // reads upright there (`/R`, in quarter turns counterclockwise). The look is drawn upright,
    // as wide as the widget is high if it lies on its side, and turned into place by its matrix.
    let turn = mk
        .and_then(|mk| mk.get(b"R").ok())
        .and_then(|r| number(resolve(doc, r)))
        .map_or(0, |r| (r as i64).rem_euclid(360));
    let (w, h) = if turn == 90 || turn == 270 {
        (h, w)
    } else {
        (w, h)
    };
    let matrix: Option<[f64; 6]> = match turn {
        90 => Some([0.0, 1.0, -1.0, 0.0, h, 0.0]),
        180 => Some([-1.0, 0.0, 0.0, -1.0, w, h]),
        270 => Some([0.0, -1.0, 1.0, 0.0, 0.0, w]),
        _ => None,
    };
    let part = |key: &[u8]| {
        mk.and_then(|mk| mk.get(key).ok())
            .and_then(|c| color(&numbers(doc, c)))
    };
    let (background, border) = (part(b"BG"), part(b"BC"));
    let border_width = dict
        .get(b"BS")
        .ok()
        .and_then(|bs| resolve(doc, bs).as_dict().ok())
        .and_then(|bs| bs.get(b"W").ok())
        .and_then(|w| number(resolve(doc, w)))
        .unwrap_or(1.0)
        .clamp(0.0, 12.0);
    let border_width = if border.is_some() { border_width } else { 0.0 };

    let rgb = |c: Color| format!("{} {} {}", n(c.r.into()), n(c.g.into()), n(c.b.into()));
    let mut content = String::new();
    if let Some(c) = background {
        content.push_str(&format!("{} rg\n0 0 {} {} re\nf\n", rgb(c), n(w), n(h)));
    }
    if let Some(c) = border.filter(|_| border_width > 0.0) {
        content.push_str(&format!(
            "{} RG\n{} w\n{} {} {} {} re\nS\n",
            rgb(c),
            n(border_width),
            n(border_width / 2.0),
            n(border_width / 2.0),
            n(w - border_width),
            n(h - border_width),
        ));
    }

    // The text, inside the border with a little room.
    let pad = border_width.max(1.0) + 1.0;
    let inside = (w - 2.0 * pad).max(1.0);
    let size = if look.size > 0.0 {
        look.size
    } else if look.multiline {
        12.0
    } else {
        ((h - 2.0 * pad) / 1.15).clamp(4.0, 12.0)
    };
    // Helvetica's proportions, near enough for where a line sits.
    let (ascent, descent, leading) = (0.718 * size, 0.207 * size, 1.15 * size);
    let mut marked = String::new();
    let (lines, first) = match body {
        Body::Text(text) => {
            let shown: String = if look.password {
                "*".repeat(text.chars().count())
            } else {
                (*text).to_owned()
            };
            if look.multiline {
                (wrap(&shown, size, inside, look.measure), h - pad - ascent)
            } else {
                (
                    vec![shown.replace(['\n', '\r'], " ")],
                    (h - ascent - descent) / 2.0 + descent,
                )
            }
        }
        Body::List { labels, chosen } => {
            // As many rows as fit, from the top, or from further down so that the first chosen
            // one shows.
            let rows = (((h - 2.0 * pad) / leading).floor() as usize).max(1);
            let from = chosen.first().map_or(0, |c| (c + 1).saturating_sub(rows));
            for row in chosen.iter().filter_map(|c| c.checked_sub(from)) {
                if row < rows {
                    marked.push_str(&format!(
                        "0.6 0.75 0.85 rg\n{} {} {} {} re\nf\n",
                        n(pad),
                        n(h - pad - (row + 1) as f64 * leading),
                        n(inside),
                        n(leading),
                    ));
                }
            }
            let lines = labels.iter().skip(from).take(rows).cloned().collect();
            let above = (leading - ascent - descent) / 2.0;
            (lines, h - pad - above - ascent)
        }
    };
    let font_name = if look.font.is_some() && !look.font_name.is_empty() {
        look.font_name.as_str()
    } else {
        "Helv"
    };
    let mut bytes = content.into_bytes();
    bytes.extend_from_slice(
        format!(
            "/Tx BMC\nq\n{} {} {} {} re\nW n\n{marked}BT\n/{} {} Tf\n{} rg\n",
            n(pad),
            n(pad),
            n(inside),
            n((h - 2.0 * pad).max(1.0)),
            font_name,
            n(size),
            rgb(look.color),
        )
        .as_bytes(),
    );
    let mut at = (0.0, 0.0);
    // A comb: each letter in the middle of its cell, and no more letters than cells.
    let comb = look.comb.filter(|_| matches!(body, Body::Text(_)));
    let pieces: Vec<(f64, f64, String)> = match comb {
        Some(cells) => {
            let cell = w / cells as f64;
            let line = lines.first().map_or("", String::as_str);
            line.chars()
                .take(cells)
                .enumerate()
                .map(|(i, c)| {
                    let letter = c.to_string();
                    let width = text_width(&letter, size, look.measure);
                    (i as f64 * cell + (cell - width) / 2.0, first, letter)
                })
                .collect()
        }
        None => Vec::new(),
    };
    for (x, y, letter) in &pieces {
        bytes.extend_from_slice(format!("{} {} Td\n(", n(x - at.0), n(y - at.1)).as_bytes());
        at = (*x, *y);
        for byte in encode_text(letter) {
            if matches!(byte, b'(' | b')' | b'\\') {
                bytes.push(b'\\');
            }
            bytes.push(byte);
        }
        bytes.extend_from_slice(b") Tj\n");
    }
    let lines = if comb.is_some() { Vec::new() } else { lines };
    for (i, line) in lines.iter().enumerate() {
        let width = text_width(line, size, look.measure);
        let x = match look.alignment {
            1 => pad + (inside - width) / 2.0,
            2 => pad + inside - width,
            _ => pad,
        };
        let y = first - i as f64 * leading;
        bytes.extend_from_slice(format!("{} {} Td\n(", n(x - at.0), n(y - at.1)).as_bytes());
        at = (x, y);
        for byte in encode_text(line) {
            if matches!(byte, b'(' | b')' | b'\\') {
                bytes.push(b'\\');
            }
            bytes.push(byte);
        }
        bytes.extend_from_slice(b") Tj\n");
    }
    bytes.extend_from_slice(b"ET\nQ\nEMC\n");

    let font = look.font.clone().unwrap_or_else(|| {
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        }
        .into()
    });
    let mut fonts = Dictionary::new();
    fonts.set(font_name, font);
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => vec![0.into(), 0.into(), Object::Real(w as f32), Object::Real(h as f32)],
        "Resources" => dictionary! { "Font" => fonts },
    };
    if let Some(matrix) = matrix {
        let numbers: Vec<Object> = matrix.iter().map(|&v| Object::Real(v as f32)).collect();
        dict.set("Matrix", numbers);
    }
    Some(Stream::new(dict, bytes))
}

/// A number as content streams want it: no exponent, no needless digits.
fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}
