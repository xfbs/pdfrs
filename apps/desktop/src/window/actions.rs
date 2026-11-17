//! The window's actions (what menus, shortcuts and buttons activate), the page number entry, and
//! dropping files onto the window.

use super::*;

/// Actions that need an open document.
const DOCUMENT_ACTIONS: &[&str] = &[
    "zoom-in",
    "zoom-out",
    "zoom-actual",
    "fit-width",
    "fit-page",
    "layout",
    "cover-alone",
    "trim-margins",
    "go-to-page",
    "next-page",
    "previous-page",
    "first-page",
    "last-page",
    "copy",
    "select-all",
    "find",
    "find-next",
    "find-previous",
    "rotate-left",
    "rotate-right",
    "highlight",
    "underline",
    "strike-out",
    "show-markup",
    "markup-tool",
    "markup-color",
    "save",
    "save-as",
    "insert-file",
    "delete-pages",
    "export-pages",
    "reload",
    "print",
    "properties",
];

/// Actions that need a document with a form that can be filled in.
const FORM_ACTIONS: &[&str] = &["reset-form", "highlight-fields"];

impl Window {
    /// Activates `action` on the focused text field, if one has the focus.
    pub(super) fn forward_to_entry(&self, action: &str) -> bool {
        let Some(focus) = gtk::prelude::GtkWindowExt::focus(&self.0.window) else {
            return false;
        };
        // The text field for typing on the page: its own keys do these, if they get to it.
        if let Some(view) = focus.downcast_ref::<gtk::TextView>() {
            let buffer = view.buffer();
            match action {
                "selection.select-all" => {
                    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
                }
                "clipboard.copy" => buffer.copy_clipboard(&view.clipboard()),
                "text.undo" => buffer.undo(),
                "text.redo" => buffer.redo(),
                _ => return false,
            }
            return true;
        }
        if focus.downcast_ref::<gtk::Text>().is_none() {
            return false;
        }
        focus.activate_action(action, None).is_ok()
    }

    /// Updates what depends on the edits: Undo and Redo, the title, the page count.
    pub(super) fn update_edit_state(&self) {
        self.refresh_markup_list();
        let inner = &self.0;
        let Some(doc) = inner.document.borrow().clone() else {
            return;
        };
        let d = doc.borrow();
        let (undo, redo) = (d.edit().undo_edit(), d.edit().redo_edit());
        for (name, edit) in [("undo", undo), ("redo", redo)] {
            if let Some(a) = inner
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                a.set_enabled(edit.is_some());
            }
        }
        let section = &inner.undo_section;
        section.remove_all();
        section.append(
            Some(&undo.map_or_else(|| gettext("Undo"), undo_label)),
            Some("win.undo"),
        );
        section.append(
            Some(&redo.map_or_else(|| gettext("Redo"), redo_label)),
            Some("win.redo"),
        );

        let pages = d.viewer.page_count();
        let subtitle = if d.edit().is_modified() {
            ngettext_f("{n} page — Edited", "{n} pages — Edited", pages, &[])
        } else {
            ngettext_f("{n} page", "{n} pages", pages, &[])
        };
        inner.title.set_subtitle(&subtitle);
        // With labels, `show_page_number` gives the position instead.
        if d.labels().is_none() {
            // Translators: follows the box with the page number: "3 of 120".
            let total = gettext_f("of {pages}", &[("pages", &pages)]);
            inner.page_total.set_label(&total);
        }
    }

    /// Enables Back and Forward where there is a place to go to.
    pub(super) fn update_history_actions(&self) {
        let (back, forward) = (self.0.canvas)
            .with_viewer(|v| (v.can_go_back(), v.can_go_forward()))
            .unwrap_or_default();
        for (name, enabled) in [("go-back", back), ("go-forward", forward)] {
            if let Some(a) = (self.0.window.lookup_action(name)).and_downcast::<gio::SimpleAction>()
            {
                a.set_enabled(enabled);
            }
        }
    }

    pub(super) fn set_document_actions_enabled(&self, enabled: bool) {
        for w in &self.0.document_widgets {
            w.set_visible(enabled);
        }
        for name in DOCUMENT_ACTIONS {
            if let Some(a) = self
                .0
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                a.set_enabled(enabled);
            }
        }
        self.update_history_actions();
        // Those for a form only where there is one.
        let form = enabled
            && (self.0.document.borrow().as_ref()).is_some_and(|d| d.borrow_mut().has_form());
        for name in FORM_ACTIONS {
            if let Some(a) = self
                .0
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                a.set_enabled(form);
            }
        }
        if enabled {
            self.update_edit_state();
        } else {
            for name in ["undo", "redo"] {
                if let Some(a) = self
                    .0
                    .window
                    .lookup_action(name)
                    .and_downcast::<gio::SimpleAction>()
                {
                    a.set_enabled(false);
                }
            }
        }
    }

    pub(super) fn setup_actions(&self) {
        let window = &self.0.window;
        let weak = Rc::downgrade(&self.0);
        let with = move |f: fn(&Window)| {
            let weak = weak.clone();
            move |_: &adw::ApplicationWindow, _: &gio::SimpleAction, _: Option<&glib::Variant>| {
                if let Some(inner) = weak.upgrade() {
                    f(&Window(inner));
                }
            }
        };

        window.add_action_entries([
            gio::ActionEntry::builder("open")
                .activate(with(|w| w.choose_file()))
                .build(),
            gio::ActionEntry::builder("zoom-in")
                .activate(with(|w| {
                    w.0.canvas.update_viewer(|v| v.zoom_by(1.25, centre(v)))
                }))
                .build(),
            gio::ActionEntry::builder("zoom-out")
                .activate(with(|w| {
                    w.0.canvas.update_viewer(|v| v.zoom_by(0.8, centre(v)))
                }))
                .build(),
            gio::ActionEntry::builder("zoom-actual")
                .activate(with(|w| {
                    w.0.canvas.update_viewer(|v| v.set_zoom(1.0, centre(v)))
                }))
                .build(),
            gio::ActionEntry::builder("fit-width")
                .activate(with(|w| {
                    w.0.canvas
                        .update_viewer(|v| v.set_zoom_mode(ZoomMode::FitWidth))
                }))
                .build(),
            gio::ActionEntry::builder("fit-page")
                .activate(with(|w| {
                    w.0.canvas
                        .update_viewer(|v| v.set_zoom_mode(ZoomMode::FitPage))
                }))
                .build(),
            gio::ActionEntry::builder("go-to-page")
                .activate(with(|w| {
                    w.0.page_entry.grab_focus();
                }))
                .build(),
            // Copy and Select All act on the page entry when it has the focus (application
            // shortcuts would otherwise take its Ctrl+C), and on the document's text otherwise.
            gio::ActionEntry::builder("copy")
                .activate(with(|w| {
                    if !w.forward_to_entry("clipboard.copy") {
                        w.0.canvas.copy();
                    }
                }))
                .build(),
            gio::ActionEntry::builder("select-all")
                .activate(with(|w| {
                    if w.forward_to_entry("selection.select-all") {
                        return;
                    }
                    match w.0.sidebar.borrow().as_ref() {
                        Some(s) if w.sidebar_has_focus() => s.select_all(),
                        _ => w.0.canvas.select_all(),
                    }
                }))
                .build(),
            gio::ActionEntry::builder("find")
                .activate(with(|w| {
                    let inner = &w.0;
                    inner.search_bar.set_search_mode(true);
                    inner.search_entry.grab_focus();
                    inner.search_entry.select_region(0, -1);
                }))
                .build(),
            gio::ActionEntry::builder("find-next")
                .activate(with(|w| w.search_step(true)))
                .build(),
            gio::ActionEntry::builder("find-previous")
                .activate(with(|w| w.search_step(false)))
                .build(),
            gio::ActionEntry::builder("save")
                .activate(with(|w| {
                    let w = w.clone();
                    glib::spawn_future_local(async move {
                        w.save().await;
                    });
                }))
                .build(),
            gio::ActionEntry::builder("save-as")
                .activate(with(|w| {
                    let w = w.clone();
                    glib::spawn_future_local(async move { w.save_as().await });
                }))
                .build(),
            gio::ActionEntry::builder("insert-file")
                .activate(with(|w| {
                    let w = w.clone();
                    glib::spawn_future_local(async move { w.choose_insert_file().await });
                }))
                .build(),
            gio::ActionEntry::builder("print")
                .activate(with(|w| w.print(None)))
                .build(),
            gio::ActionEntry::builder("properties")
                .activate(with(Window::show_properties))
                .build(),
            gio::ActionEntry::builder("reload")
                .activate(with(|w| {
                    let w = w.clone();
                    glib::spawn_future_local(async move { w.reload().await });
                }))
                .build(),
            gio::ActionEntry::builder("export-pages")
                .activate(with(|w| {
                    let w = w.clone();
                    glib::spawn_future_local(async move { w.export_pages().await });
                }))
                .build(),
            gio::ActionEntry::builder("delete-pages")
                .activate(with(|w| w.delete_pages()))
                .build(),
            gio::ActionEntry::builder("highlight")
                .activate(with(|w| w.mark_once(MarkupStyle::Highlight)))
                .build(),
            gio::ActionEntry::builder("underline")
                .activate(with(|w| w.mark_once(MarkupStyle::Underline)))
                .build(),
            gio::ActionEntry::builder("strike-out")
                .activate(with(|w| w.mark_once(MarkupStyle::StrikeOut)))
                .build(),
            gio::ActionEntry::builder("rotate-left")
                .activate(with(|w| w.rotate(Rotation::Cw270)))
                .build(),
            gio::ActionEntry::builder("rotate-right")
                .activate(with(|w| w.rotate(Rotation::Cw90)))
                .build(),
            gio::ActionEntry::builder("undo")
                .activate(with(|w| {
                    if !w.forward_to_entry("text.undo") {
                        w.edit(|d| d.undo().map(|_| Edited::default()));
                    }
                }))
                .build(),
            gio::ActionEntry::builder("redo")
                .activate(with(|w| {
                    if !w.forward_to_entry("text.redo") {
                        w.edit(|d| d.redo().map(|_| Edited::default()));
                    }
                }))
                .build(),
            gio::ActionEntry::builder("next-page")
                .activate(with(|w| w.0.canvas.update_viewer(Viewer::next_page)))
                .build(),
            gio::ActionEntry::builder("previous-page")
                .activate(with(|w| w.0.canvas.update_viewer(Viewer::previous_page)))
                .build(),
            gio::ActionEntry::builder("first-page")
                .activate(with(|w| w.0.canvas.jump(|v| v.go_to_page(0))))
                .build(),
            gio::ActionEntry::builder("last-page")
                .activate(with(|w| {
                    w.0.canvas
                        .jump(|v| v.go_to_page(v.page_count().saturating_sub(1)))
                }))
                .build(),
            gio::ActionEntry::builder("go-back")
                .activate(with(|w| {
                    w.0.canvas.update_viewer(|v| {
                        v.go_back();
                    })
                }))
                .build(),
            gio::ActionEntry::builder("go-forward")
                .activate(with(|w| {
                    w.0.canvas.update_viewer(|v| {
                        v.go_forward();
                    })
                }))
                .build(),
        ]);

        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("layout")
            .parameter_type(Some(&String::static_variant_type()))
            .state("continuous".to_variant())
            .activate(move |_, action, param| {
                let (Some(inner), Some(name)) =
                    (weak.upgrade(), param.and_then(|p| p.get::<String>()))
                else {
                    return;
                };
                let Some(mode) = LayoutMode::from_name(&name) else {
                    return;
                };
                action.set_state(&name.to_variant());
                inner.canvas.update_viewer(|v| v.set_layout_mode(mode));
            })
            .build()]);
        // The markup bar: the tool pressed in ("none" for none, and activating the one that is in
        // lets go of it) and the color shown ("other" for a picked annotation in a color the bar
        // doesn't have).
        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("markup-tool")
            .parameter_type(Some(&String::static_variant_type()))
            .state("none".to_variant())
            .activate(move |_, action, param| {
                let (Some(inner), Some(name)) =
                    (weak.upgrade(), param.and_then(|p| p.get::<String>()))
                else {
                    return;
                };
                let w = Window(inner);
                let current = action.state().and_then(|s| s.get::<String>());
                let tool = tool_named(&name).filter(|_| current.as_deref() != Some(&name));
                // Taking up a tool is to make something new: what was picked is let go of, and the
                // bar shows the tool's color.
                if tool.is_some() {
                    w.0.canvas.unpick_annotation();
                    w.0.placing.replace(None);
                }
                w.set_markup_tool(tool);
                // What is selected already is marked at once.
                if let Some(Tool::Mark(style)) = tool {
                    w.mark(style);
                }
            })
            .change_state(|_, action, state| {
                if let Some(state) = state {
                    action.set_state(state);
                }
            })
            .build()]);
        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("markup-color")
            .parameter_type(Some(&String::static_variant_type()))
            .state("yellow".to_variant())
            .activate(move |_, _, param| {
                let (Some(inner), Some(name)) =
                    (weak.upgrade(), param.and_then(|p| p.get::<String>()))
                else {
                    return;
                };
                let known = markup_colors().into_iter().find(|(n, _, _)| *n == name);
                if let Some((name, _, _)) = known {
                    Window(inner).choose_markup_color(name);
                }
            })
            .change_state(|_, action, state| {
                if let Some(state) = state {
                    action.set_state(state);
                }
            })
            .build()]);
        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("cover-alone")
            .state(false.to_variant())
            .activate(move |_, action, _| {
                let Some(inner) = weak.upgrade() else { return };
                let on = !action
                    .state()
                    .and_then(|s| s.get::<bool>())
                    .unwrap_or(false);
                action.set_state(&on.to_variant());
                inner.canvas.update_viewer(|v| v.set_cover_alone(on));
            })
            .build()]);

        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("trim-margins")
            .state(false.to_variant())
            .activate(move |_, action, _| {
                let Some(inner) = weak.upgrade() else { return };
                let on = !action
                    .state()
                    .and_then(|s| s.get::<bool>())
                    .unwrap_or(false);
                Window(inner).set_trim_margins(on);
            })
            .build()]);

        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("highlight-fields")
            .state(false.to_variant())
            .activate(move |_, action, _| {
                let Some(inner) = weak.upgrade() else { return };
                let on = !action
                    .state()
                    .and_then(|s| s.get::<bool>())
                    .unwrap_or(false);
                action.set_state(&on.to_variant());
                inner.canvas.set_highlight_fields(on);
            })
            .build()]);
        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("reset-form")
            .activate(move |_, _, _| {
                let Some(inner) = weak.upgrade() else { return };
                let window = Window(inner);
                window.finish_text_entry();
                window.annotate(|d| d.reset_form());
            })
            .build()]);

        // Which sidebar page to show: "thumbnails", "outline" or "markup".
        let weak = Rc::downgrade(&self.0);
        window.add_action_entries([gio::ActionEntry::builder("sidebar-page")
            .parameter_type(Some(&String::static_variant_type()))
            .state("thumbnails".to_variant())
            .activate(move |_, action, param| {
                let (Some(inner), Some(page)) =
                    (weak.upgrade(), param.and_then(|p| p.get::<String>()))
                else {
                    return;
                };
                if let Some(stack) = inner.sidebar_stack.borrow().as_ref()
                    && stack.child_by_name(&page).is_some()
                {
                    stack.set_visible_child_name(&page);
                    inner.split.set_show_sidebar(true);
                    action.set_state(&page.to_variant());
                }
            })
            .build()]);

        window.add_action(&gio::PropertyAction::new(
            "show-sidebar",
            &self.0.split,
            "show-sidebar",
        ));
        window.add_action(&gio::PropertyAction::new(
            "show-markup",
            &self.0.markup_bar,
            "reveal-child",
        ));
    }

    /// Puts page `page`'s label (or number) into the page entry, plus "(3 of 120)" when the
    /// document has labels.
    pub(super) fn show_page_number(&self, page: usize) {
        let inner = &self.0;
        let Some(doc) = inner.document.borrow().clone() else {
            return;
        };
        let d = doc.borrow();
        inner.page_entry.set_text(&d.page_label(page));
        if d.labels().is_some() {
            let total = gettext_f(
                // Translators: follows the box with a printed page number:
                // "iv (4 of 120)".
                "({position} of {pages})",
                &[("position", &(page + 1)), ("pages", &d.viewer.page_count())],
            );
            inner.page_total.set_label(&total);
        }
    }

    pub(super) fn setup_page_entry(&self) {
        let entry = &self.0.page_entry;
        let weak = Rc::downgrade(&self.0);
        entry.connect_activate(move |e| {
            let Some(inner) = weak.upgrade() else { return };
            let target = inner
                .document
                .borrow()
                .as_ref()
                .and_then(|d| d.borrow().find_page(&e.text()));
            match target {
                Some(page) => {
                    inner.canvas.jump(|v| v.go_to_page(page));
                    inner.canvas.grab_focus();
                }
                None => {
                    e.error_bell();
                    let current = inner.last_page.get();
                    Window(inner).show_page_number(current);
                }
            }
        });
        // Restore the current page number when focus leaves without Enter.
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(&self.0);
        focus.connect_leave(move |_| {
            if let Some(inner) = weak.upgrade() {
                let current = inner.last_page.get();
                if current != usize::MAX {
                    Window(inner).show_page_number(current);
                }
            }
        });
        entry.add_controller(focus);
    }

    pub(super) fn setup_drop(&self) {
        let target = gtk::DropTarget::new(gio::File::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(&self.0);
        target.connect_drop(move |_, value, _, _| {
            let (Some(inner), Ok(file)) = (weak.upgrade(), value.get::<gio::File>()) else {
                return false;
            };
            Window(inner).open(file);
            true
        });
        self.0
            .window
            .content()
            .expect("content")
            .add_controller(target);
    }
}
