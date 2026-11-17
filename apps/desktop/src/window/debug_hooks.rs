//! The test hooks of a window, described in `debug.rs`.

use super::*;

thread_local! {
    /// The windows, with the test hooks only: for those that reach into another window.
    pub(super) static DEBUG_WINDOWS: RefCell<Vec<std::rc::Weak<Inner>>> = const { RefCell::new(Vec::new()) };
}

impl Window {
    /// The test hooks described in `debug.rs`.
    pub(super) fn setup_debug_actions(&self) {
        let string = String::static_variant_type();
        let action = |name: &str, f: fn(&Window, &str)| {
            let weak = Rc::downgrade(&self.0);
            gio::ActionEntry::builder(name)
                .parameter_type(Some(&string))
                .activate(move |_: &adw::ApplicationWindow, _, param| {
                    if let (Some(inner), Some(arg)) =
                        (weak.upgrade(), param.and_then(|p| p.get::<String>()))
                    {
                        f(&Window(inner), &arg);
                    }
                })
                .build()
        };
        self.0.window.add_action_entries([
            action("debug-state", |w, path| {
                if let Err(e) = std::fs::write(path, w.debug_state()) {
                    eprintln!("debug-state: {path}: {e}");
                }
            }),
            action("debug-screenshot", |w, path| {
                debug::screenshot(w.0.window.upcast_ref(), path)
            }),
            action("debug-maximize", |w, _| w.0.window.maximize()),
            // Save As without the file dialog.
            action("debug-save-as", |w, path| {
                let (w, file) = (w.clone(), gio::File::for_path(path));
                glib::spawn_future_local(async move {
                    w.save_as_file(file).await;
                });
            }),
            // Drops thumbnails without dragging: "0,1@450" drops pages 0 and 1 at window height
            // 450.
            action("debug-drop-thumbnails", |w, arg| {
                let Some((pages, y)) = arg.split_once('@') else {
                    return;
                };
                let pages = pages.split(',').filter_map(|p| p.parse().ok()).collect();
                if let (Some(s), Ok(y)) = (w.0.sidebar.borrow().as_ref(), y.parse()) {
                    s.drop_at(pages, &w.0.window, y);
                }
            }),
            // Drops files on the sidebar without dragging: "a.pdf|b.pdf@450" drops two files at
            // window height 450.
            action("debug-drop-files", |w, arg| {
                let Some((paths, y)) = arg.rsplit_once('@') else {
                    return;
                };
                let files = paths.split('|').map(gio::File::for_path).collect();
                if let (Some(s), Ok(y)) = (w.0.sidebar.borrow().as_ref(), y.parse()) {
                    s.drop_files_at(files, &w.0.window, y);
                }
            }),
            // Drops pages of another window without dragging: "1:0,2@450" drops pages 0 and 2 of
            // window 1 at window height 450.
            action("debug-drop-pages-from", |w, arg| {
                let parse = || {
                    let (window, rest) = arg.split_once(':')?;
                    let (pages, y) = rest.rsplit_once('@')?;
                    let pages: Vec<usize> =
                        pages.split(',').filter_map(|p| p.parse().ok()).collect();
                    Some((window.parse::<u32>().ok()?, pages, y.parse::<f64>().ok()?))
                };
                let Some((n, pages, y)) = parse() else {
                    return;
                };
                let from = DEBUG_WINDOWS.with_borrow(|ws| {
                    ws.iter()
                        .filter_map(std::rc::Weak::upgrade)
                        .find(|i| i.window.id() == n)
                });
                let Some(doc) = from.and_then(|i| i.document.borrow().clone()) else {
                    return;
                };
                let bundle = doc.borrow().bundle(&pages);
                if let Some(s) = w.0.sidebar.borrow().as_ref() {
                    s.drop_bundle_at(bundle, &w.0.window, y);
                }
            }),
            // Prints to a PDF file without the dialog: "out.pdf", or "out.pdf|selection" for the
            // pages selected in the sidebar.
            action("debug-print-to-file", |w, arg| {
                let (path, selection) = match arg.strip_suffix("|selection") {
                    Some(p) => (p, true),
                    None => (arg, false),
                };
                w.print(Some((path.into(), selection)));
            }),
            // Export Selected Pages without the file dialog.
            action("debug-export", |w, path| {
                let (w, file) = (w.clone(), gio::File::for_path(path));
                glib::spawn_future_local(async move {
                    let pages = w.pages_to_export();
                    w.export_to(&pages, file).await;
                });
            }),
            // Insert Pages from File without the file dialog.
            action("debug-insert", |w, path| {
                let (w, file) = (w.clone(), gio::File::for_path(path));
                glib::spawn_future_local(async move {
                    w.insert_file(file).await;
                });
            }),
            // Chooses the saved signature of that number, from 0, to be placed, as its row under
            // the signature button does.
            action("debug-place-signature", |w, number| {
                let saved = signatures::saved();
                if let Some((_, signature)) =
                    number.parse::<usize>().ok().and_then(|i| saved.get(i))
                {
                    w.place_signature(signature.clone());
                }
            }),
            // Sets the text controls of the markup bar, as by hand: words like "serif bold italic
            // 18".
            action("debug-text-style", |w, words| {
                for word in words.split_ascii_whitespace() {
                    match word {
                        "sans" => w.0.text_font.set_selected(0),
                        "serif" => w.0.text_font.set_selected(1),
                        "mono" => w.0.text_font.set_selected(2),
                        "bold" => w.0.text_bold.set_active(true),
                        "italic" => w.0.text_italic.set_active(true),
                        "regular" => {
                            w.0.text_bold.set_active(false);
                            w.0.text_italic.set_active(false);
                        }
                        size => {
                            if let Ok(size) = size.parse::<f64>() {
                                w.0.text_size.set_value(size);
                            }
                        }
                    }
                }
            }),
            // Gives the keyboard to the size field of the markup bar, as a click into it does.
            action("debug-focus-text-size", |w, _| {
                w.0.text_size.grab_focus();
            }),
            // Shows the dialog for a new signature.
            action("debug-new-signature", |w, _| w.new_signature()),
            // Activates a row of the markup list, by its number from 0.
            action("debug-markup-row", |w, row| {
                let at = row.parse::<usize>().ok().and_then(|i| {
                    let list = w.0.markup_list.borrow();
                    list.as_ref()?.entries().get(i).map(|e| e.at)
                });
                if let Some((position, id)) = at {
                    w.0.canvas.pick_and_show(position, id);
                }
            }),
            // Answers the open "Save changes?" dialog.
            action("debug-alert-response", |w, response| {
                if let Some(alert) = w.0.alert.borrow().clone() {
                    alert.emit_by_name::<()>("response", &[&response]);
                    alert.force_close();
                }
            }),
            action("debug-go-to", |w, text| {
                let entry = &w.0.page_entry;
                entry.grab_focus();
                entry.set_text(text);
                entry.emit_activate();
            }),
        ]);
    }

    /// The window's state as JSON, for tests.
    pub(super) fn debug_state(&self) -> String {
        let inner = &self.0;
        let mut json = debug::Json::default();
        json.str(
            "view",
            &inner.stack.visible_child_name().unwrap_or_default(),
        )
        .str("title", &inner.window.title().unwrap_or_default())
        .str("toast", &inner.last_toast.borrow())
        // The title of the dialog over the window, "About" for instance.
        .str(
            "dialog",
            &inner
                .window
                .visible_dialog()
                .map(|d| d.title().to_string())
                .unwrap_or_default(),
        )
        .bool("sidebar", inner.split.shows_sidebar())
        .str(
            "sidebar_page",
            &inner
                .sidebar_stack
                .borrow()
                .as_ref()
                .and_then(|s| s.visible_child_name())
                .unwrap_or_default(),
        )
        .raw("recent", &{
            // [title, subtitle] per row of the start page's recent files.
            let mut rows = Vec::new();
            let mut child = inner.recent_list.first_child();
            while let Some(c) = child {
                if let Some(row) = c.downcast_ref::<adw::ActionRow>() {
                    rows.push(format!(
                        "[{}, {}]",
                        debug::quote(&row.title()),
                        debug::quote(&row.subtitle().unwrap_or_default())
                    ));
                }
                child = c.next_sibling();
            }
            format!("[{}]", rows.join(", "))
        })
        .str("page_entry", &inner.page_entry.text())
        .str("page_total", &inner.page_total.label())
        .str("zoom_label", &inner.zoom_button.label().unwrap_or_default())
        .num("scale", inner.canvas.device_scale())
        .bool("complete", inner.canvas.is_complete())
        .bool("maximized", inner.window.is_maximized())
        .str("cursor", inner.canvas.cursor_name().unwrap_or(""))
        // The preview of a link's target, while one shows: its size, and how many of its pixels
        // are dark.
        .raw(
            "link_preview",
            &(inner.canvas.link_preview_shown()).map_or_else(
                || "[]".to_owned(),
                |(w, h, dark)| format!("[{w}, {h}, {dark}]"),
            ),
        )
        .str(
            "focus",
            &gtk::prelude::GtkWindowExt::focus(&inner.window)
                .map_or_else(String::new, |w| w.type_().name().to_owned()),
        )
        .str(
            "selection",
            &inner.canvas.selected_text().unwrap_or_default(),
        )
        .bool("markup_bar", inner.markup_bar.reveals_child())
        .num("signatures", signatures::saved().len() as f64)
        // The text controls: whether they show, and what they are set to, such as "serif bold 18".
        .bool("text_controls", inner.text_controls.reveals_child())
        .str("text_style", &{
            let (size, font) = self.text_controls_style();
            let mut words = vec![
                match font.family {
                    FontFamily::Sans => "sans",
                    FontFamily::Serif => "serif",
                    FontFamily::Mono => "mono",
                }
                .to_owned(),
            ];
            if font.bold {
                words.push("bold".to_owned());
            }
            if font.italic {
                words.push("italic".to_owned());
            }
            words.push(format!("{size}"));
            words.join(" ")
        })
        .bool("placing_signature", inner.placing.borrow().is_some())
        .str(
            "markup_tool",
            &inner
                .window
                .action_state("markup-tool")
                .and_then(|s| s.get::<String>())
                .unwrap_or_default(),
        )
        .str(
            "markup_color",
            &inner
                .window
                .action_state("markup-color")
                .and_then(|s| s.get::<String>())
                .unwrap_or_default(),
        )
        .bool("search_bar", inner.search_bar.is_search_mode())
        // Mapped once the bar has started sliding in. Keys before that are dropped (GTK doesn't
        // deliver them to unmapped widgets).
        .bool("search_ready", inner.search_entry.is_mapped())
        .str("search_query", &inner.search_entry.text())
        .str("search_status", &inner.search_status.label());
        if let Some(s) = inner
            .document
            .borrow()
            .as_ref()
            .and_then(|d| d.borrow().search_status())
        {
            json.num("matches", s.matches as f64)
                .num("match", s.current.map_or(-1.0, |c| c as f64))
                .num("indexed", s.pages_indexed as f64);
        }
        if let Some(m) = inner
            .document
            .borrow()
            .as_ref()
            .and_then(|d| d.borrow().current_match())
        {
            json.num("match_page", m.page as f64);
        }
        // Where the canvas is in the window, and for the pages around the current one, the affine
        // map from PDF points to window coordinates: [page, a, b, c, d, e, f] with x' = a x + c y +
        // e, y' = b x + d y + f.
        let origin = inner
            .canvas
            .compute_point(&inner.window, &gtk::graphene::Point::new(0.0, 0.0))
            .map_or((0.0, 0.0), |p| (p.x() as f64, p.y() as f64));
        let (px, py) = inner.canvas.pointer();
        json.raw("canvas", &format!("[{}, {}]", origin.0, origin.1))
            .raw("pointer", &format!("[{px}, {py}]"));
        // Thumbnails on screen, [page, x, y, width, height] in the window, and the pages selected
        // in the sidebar.
        if let Some(s) = inner.sidebar.borrow().as_ref() {
            let shown: Vec<String> = s
                .visible(&inner.window)
                .iter()
                .map(|(p, r)| format!("[{p}, {}, {}, {}, {}]", r.x(), r.y(), r.width(), r.height()))
                .collect();
            let selected: Vec<String> = s.selected().iter().map(usize::to_string).collect();
            json.raw("thumbnails", &format!("[{}]", shown.join(", ")))
                .raw("thumbnails_selected", &format!("[{}]", selected.join(", ")));
        }
        let maps = inner.canvas.with_viewer(|v| {
            let current = v.current_page();
            (current.saturating_sub(2)..(current + 4).min(v.page_count()))
                .filter_map(|i| {
                    let at = |x, y| v.pdf_to_view(i, specto_geom::PdfPoint::new(x, y));
                    let (o, ex, ey) = (at(0.0, 0.0)?, at(1.0, 0.0)?, at(0.0, 1.0)?);
                    Some(format!(
                        "[{i}, {}, {}, {}, {}, {}, {}]",
                        ex.x - o.x,
                        ex.y - o.y,
                        ey.x - o.x,
                        ey.y - o.y,
                        o.x + origin.0,
                        o.y + origin.1
                    ))
                })
                .collect::<Vec<_>>()
                .join(", ")
        });
        json.raw("pages", &format!("[{}]", maps.unwrap_or_default()));
        let width = inner.window.width();
        json.num("width", width as f64)
            .num("height", inner.window.height() as f64);
        if let Some(doc) = inner.document.borrow().as_ref() {
            let d = doc.borrow();
            let pages: Vec<String> = d
                .edit()
                .pages()
                .iter()
                .map(|p| format!("[{}, {}, {}]", p.page, p.rotation.degrees(), p.source.0))
                .collect();
            // [page of the file, rotation in degrees, file] per position.
            json.raw("edit_pages", &format!("[{}]", pages.join(", ")))
                .bool("trim_margins", d.trim_margins())
                // Whether the Trim Margins switch can be used.
                .bool(
                    "trim_enabled",
                    (inner.window.lookup_action("trim-margins")).is_some_and(|a| a.is_enabled()),
                )
                .bool("modified", d.edit().is_modified())
                .num("cache_tiles", d.cache.len() as f64)
                .num("cache_bytes", d.cache.bytes() as f64)
                .bool("saving", inner.saving.get())
                .str(
                    "alert",
                    &inner
                        .alert
                        .borrow()
                        .as_ref()
                        .map(|a| a.heading().unwrap_or_default().to_string())
                        .unwrap_or_default(),
                )
                .str("subtitle", &inner.title.subtitle())
                .num("printed", f64::from(inner.printed.get()))
                // Whether there is a place to go back to, and one to go forward to.
                .raw(
                    "history",
                    &format!(
                        "[{}, {}]",
                        d.viewer.can_go_back(),
                        d.viewer.can_go_forward()
                    ),
                )
                .raw("properties", &{
                    let rows: Vec<String> = (inner.properties.borrow().iter())
                        .map(|(label, value)| {
                            format!("[{}, {}]", debug::quote(label), debug::quote(value))
                        })
                        .collect();
                    format!("[{}]", rows.join(", "))
                })
                .str(
                    "banner",
                    &if inner.banner.is_revealed() {
                        inner.banner.title().to_string()
                    } else {
                        String::new()
                    },
                )
                .str(
                    "undo",
                    &d.edit().undo_edit().map(edit_name).unwrap_or_default(),
                )
                .str(
                    "redo",
                    &d.edit().redo_edit().map(edit_name).unwrap_or_default(),
                );
        }
        // What the form's fields hold: [name, value] each, a ticked box "on", an unticked one "".
        if let Some(doc) = inner.document.borrow().as_ref() {
            let mut d = doc.borrow_mut();
            let fields: Vec<String> = d
                .form_fields()
                .iter()
                .map(|f| {
                    let value = match d.field_value(f.id) {
                        Some(specto_doc::FieldValue::Text(text)) => text,
                        Some(specto_doc::FieldValue::Check(true)) => "on".to_owned(),
                        Some(specto_doc::FieldValue::Choice(Some(choice))) => choice,
                        Some(specto_doc::FieldValue::Several(all)) => all.join(","),
                        _ => String::new(),
                    };
                    format!("[{}, {}]", debug::quote(&f.name), debug::quote(&value))
                })
                .collect();
            json.raw("fields", &format!("[{}]", fields.join(", ")));
            // The name of the field that has the keyboard.
            let focus = d.field_focus();
            let fields = d.form_fields();
            let name = fields
                .iter()
                .find(|f| Some(f.id) == focus)
                .map(|f| f.name.as_str());
            json.str("field_focus", name.unwrap_or_default());
            // What the list of a choice field offers, while it shows.
            let choices: Vec<String> = (inner.choices.borrow().iter())
                .flat_map(|c| c.labels.iter().map(|l| debug::quote(l)))
                .collect();
            json.raw("choices", &format!("[{}]", choices.join(", ")));
            if let Some(choices) = inner.choices.borrow().as_ref() {
                // What stands in the line to type a choice of one's own.
                if let Some(entry) = &choices.entry {
                    json.str("choice_typed", &entry.text());
                }
                // Which rows of a list that takes several are ticked.
                let ticked: Vec<String> = (choices.ticks.iter().enumerate())
                    .filter(|(_, tick)| tick.is_active())
                    .map(|(i, _)| i.to_string())
                    .collect();
                json.raw("choices_ticked", &format!("[{}]", ticked.join(", ")));
            }
        }
        // The annotations of the current page (the file's and new ones), and whether one is picked.
        if let Some(doc) = inner.document.borrow().clone() {
            let mut d = doc.borrow_mut();
            let page = d.viewer.current_page();
            let count = d.annotations(page).len();
            json.num("annotations", count as f64)
                .bool("annotation_picked", d.picked.is_some())
                .str("annotation_text", &d.picked_text().unwrap_or_default())
                .bool(
                    "text_editor",
                    inner.text_editor.borrow().is_some() || inner.typing.borrow().is_some(),
                );
            // The text field for typing on the page: where the overlay is asked to put it (left,
            // top, width) and the size it has.
            if let Some(t) = inner.typing.borrow().as_ref() {
                let (x, y, w) = t.area.get();
                json.raw(
                    "typing",
                    &format!("[{x}, {y}, {w}, {}, {}]", t.view.width(), t.view.height()),
                );
                json.str("typing_text", &t.text());
                json.bool("typing_hidden", matches!(t.input, TypingInput::Secret(_)));
            }
            // The rows of the markup list: [page, kind, text] each.
            let rows: Vec<String> = inner
                .markup_list
                .borrow()
                .as_ref()
                .map(MarkupList::entries)
                .unwrap_or_default()
                .iter()
                .map(|e| {
                    format!(
                        "[{}, {}, {}]",
                        debug::quote(&e.page),
                        debug::quote(&e.kind),
                        debug::quote(&e.text)
                    )
                })
                .collect();
            json.raw("markup_list", &format!("[{}]", rows.join(", ")));
            // Where its rows are in the window: [left, top, width, height] each.
            let bounds: Vec<String> = inner
                .markup_list
                .borrow()
                .as_ref()
                .map(|list| list.row_bounds(&inner.window))
                .unwrap_or_default()
                .iter()
                .map(|[x, y, w, h]| format!("[{x}, {y}, {w}, {h}]"))
                .collect();
            json.raw("markup_rows", &format!("[{}]", bounds.join(", ")));
        }
        inner.canvas.with_viewer(|v| {
            json.num("page", v.current_page() as f64)
                .num("page_count", v.page_count() as f64)
                .str("layout", v.layout_mode().name())
                .bool("cover_alone", v.settings().cover_alone)
                .str("zoom_mode", v.zoom_mode().name())
                .num("zoom", v.zoom());
            // The current page as shown, in points.
            if let Some(p) = v.pages().get(v.current_page()) {
                let size = p.displayed().size();
                json.raw("page_size", &format!("[{}, {}]", size.width, size.height));
            }
        });
        json.finish()
    }
}
