//! A document window: header bar, thumbnail sidebar and page view.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use specto_annot::{AnnotType, Color, FontFamily, MarkupStyle, TextFont};
use specto_doc::{Edit, FieldId, Rotation, SourceId};
use specto_geom::ViewPoint;
use specto_render::{OpenError, PdfData};
use specto_session::{DraftKind, FieldClick, TextBoxPlace};
use specto_view::{LayoutMode, Viewer, ZoomMode};

use crate::canvas::{Canvas, FieldKey};
use crate::debug;
use crate::document::{Document, Indexed, MAIN, PageBundle, SaveJob, SharedDocument};
use crate::i18n::{gettext, gettext_f, ngettext_f};
use crate::markup_list::{Entry, MarkupList};
use crate::outline::OutlineView;
use crate::print;
use crate::properties;
use crate::signatures::{self, Signature};
use crate::state;
use crate::thumbnails::Sidebar;

mod actions;
mod debug_hooks;
mod files;
mod markup;
mod pages;
mod search;

use debug_hooks::*;
use files::*;
use markup::*;
use pages::*;

#[derive(Clone)]
pub struct Window(Rc<Inner>);

pub struct Inner {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    split: adw::OverlaySplitView,
    title: adw::WindowTitle,
    canvas: Canvas,
    page_entry: gtk::Entry,
    page_total: gtk::Label,
    zoom_button: gtk::MenuButton,
    /// Header bar widgets that only make sense with a document open.
    document_widgets: Vec<gtk::Widget>,
    document: RefCell<Option<SharedDocument>>,
    /// URI of the open file, for remembering its view state.
    uri: RefCell<Option<String>>,
    recent_title: gtk::Label,
    recent_list: gtk::ListBox,
    sidebar: RefCell<Option<Sidebar>>,
    /// The sidebar's pages: Thumbnails, Contents if the document has an outline, and Markup.
    sidebar_stack: RefCell<Option<adw::ViewStack>>,
    /// The list of the document's markup in the sidebar.
    markup_list: RefCell<Option<MarkupList>>,
    last_page: Cell<usize>,
    /// The last message shown as a toast, for tests.
    last_toast: RefCell<String>,
    search_bar: gtk::SearchBar,
    /// The bar with the markup tools and colors.
    markup_bar: gtk::Revealer,
    /// The controls for how text boxes are set, on the markup bar: all of them, and each.
    text_controls: gtk::Revealer,
    text_font: gtk::DropDown,
    text_bold: gtk::ToggleButton,
    text_italic: gtk::ToggleButton,
    text_size: gtk::SpinButton,
    /// The size and font new text boxes get.
    text_style: Cell<(f64, TextFont)>,
    /// The controls are being set to show a style, not by the user.
    showing_text_style: Cell<bool>,
    /// The dot on the color button of the markup bar: the color in use.
    color_swatch: gtk::Box,
    /// Lies over the page view, for typing text onto the page.
    page_overlay: gtk::Overlay,
    /// The list of saved signatures, under its button on the markup bar.
    signature_menu: gtk::Popover,
    /// The signature that the next click on a page places, if one was chosen.
    placing: RefCell<Option<Signature>>,
    /// The text field over the page, while a text box is typed into.
    typing: RefCell<Option<Typing>>,
    /// The editor for an annotation's text, while it is open.
    text_editor: RefCell<Option<gtk::Popover>>,
    /// The list to choose from for a choice field of the form, while it shows.
    choices: RefCell<Option<Choices>>,
    /// The color of each tool of the markup bar (in the order of `TOOLS`), by name (see
    /// `markup_colors`).
    tool_colors: Cell<[&'static str; TOOLS.len()]>,
    search_entry: gtk::SearchEntry,
    /// "3 of 17", "No matches" and the like.
    search_status: gtk::Label,
    /// Undo and Redo in the main menu, named after the edit.
    undo_section: gio::Menu,
    /// A save is running in the background.
    saving: Cell<bool>,
    /// The user chose to close despite unsaved changes (or saved them).
    close_confirmed: Cell<bool>,
    /// The "Save changes?" dialog while it's open (for tests).
    alert: RefCell<Option<adw::AlertDialog>>,
    /// The file as it was when read or last saved, to tell changes by other programs from our own.
    known: RefCell<Option<Fingerprint>>,
    /// Watches the file for changes by other programs.
    monitor: RefCell<Option<gio::FileMonitor>>,
    /// A look at the file is scheduled (changes come in bursts).
    check_pending: Cell<bool>,
    /// The password the file was opened with, for reloading it.
    password: RefCell<Option<String>>,
    /// Says that the file changed on disk while there are edits, or is gone.
    banner: adw::Banner,
    /// How many print jobs finished (for tests).
    printed: Cell<u32>,
    /// The rows of the Properties dialog while it shows (for tests).
    properties: RefCell<Vec<(String, String)>>,
}

impl Window {
    pub fn new(app: &adw::Application) -> Self {
        let canvas = Canvas::new();
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&canvas)
            .hexpand(true)
            .vexpand(true)
            .build();

        // Over the page view: where text is typed onto the page (see `edit_text_box`). Above it,
        // the markup bar, added below.
        let page_overlay = gtk::Overlay::new();
        page_overlay.set_child(Some(&scrolled));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&page_overlay);
        let split = adw::OverlaySplitView::builder()
            .content(&content)
            .show_sidebar(true)
            .min_sidebar_width(170.0)
            .max_sidebar_width(260.0)
            .build();

        let empty = adw::StatusPage::builder()
            .icon_name("x-office-document-symbolic")
            .title(gettext("Open a PDF"))
            .description(gettext("Drop a file here or choose one"))
            .build();
        let open_button = gtk::Button::builder()
            .label(gettext("Open…"))
            .action_name("win.open")
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .build();
        let recent_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let recent_title = gtk::Label::builder()
            .label(gettext("Recent"))
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        let start = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .halign(gtk::Align::Center)
            .width_request(360)
            .build();
        start.append(&open_button);
        start.append(&recent_title);
        start.append(&recent_list);
        empty.set_child(Some(&start));

        let stack = gtk::Stack::new();
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&split, Some("document"));
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));

        // Header bar.
        let title = adw::WindowTitle::new(crate::APP_NAME, "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        let sidebar_toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text(gettext("Thumbnails (F9)"))
            .build();
        split
            .bind_property("show-sidebar", &sidebar_toggle, "active")
            .bidirectional()
            .sync_create()
            .build();
        header.pack_start(&sidebar_toggle);
        header.pack_start(
            &gtk::Button::builder()
                .icon_name("document-open-symbolic")
                .tooltip_text(gettext_f(
                    "Open ({shortcut})",
                    &[("shortcut", &primary("O"))],
                ))
                .action_name("win.open")
                .build(),
        );

        // The main menu: saving, printing, editing and About. Also on the start page, where only
        // About is enabled.
        let undo_section = gio::Menu::new();
        let main_menu = gio::Menu::new();
        main_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(Some(&gettext("Save")), Some("win.save"));
            m.append(Some(&gettext("Save As…")), Some("win.save-as"));
            m.append(
                Some(&gettext("Export Selected Pages…")),
                Some("win.export-pages"),
            );
            m.append(Some(&gettext("Print…")), Some("win.print"));
            m.append(Some(&gettext("Properties")), Some("win.properties"));
            m
        });
        main_menu.append_section(None, &undo_section);
        main_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(Some(&gettext("Rotate Left")), Some("win.rotate-left"));
            m.append(Some(&gettext("Rotate Right")), Some("win.rotate-right"));
            m.append(Some(&gettext("Delete Pages")), Some("win.delete-pages"));
            m
        });
        main_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(
                Some(&gettext("Insert Pages from File…")),
                Some("win.insert-file"),
            );
            m
        });
        main_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(
                Some(&gettext("Highlight Form Fields")),
                Some("win.highlight-fields"),
            );
            m.append(Some(&gettext("Reset Form")), Some("win.reset-form"));
            m
        });
        main_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(
                Some(&gettext_f("About {app}", &[("app", &crate::APP_NAME)])),
                Some("app.about"),
            );
            m
        });
        let main_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(gettext("Main Menu"))
            .menu_model(&main_menu)
            .primary(true)
            .build();
        header.pack_end(&main_button);

        let view_menu = gio::Menu::new();
        let layout_section = gio::Menu::new();
        for (label, target) in [
            (gettext("Single Page"), "single"),
            (gettext("Continuous"), "continuous"),
            (gettext("Two Pages"), "two-page"),
            (gettext("Two Pages, Continuous"), "two-page-continuous"),
        ] {
            layout_section.append(Some(&label), Some(&format!("win.layout::{target}")));
        }
        view_menu.append_section(None, &layout_section);
        view_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(
                Some(&gettext("Show Cover Page Alone")),
                Some("win.cover-alone"),
            );
            m.append(Some(&gettext("Trim Margins")), Some("win.trim-margins"));
            m
        });
        let layout_button = gtk::MenuButton::builder()
            .icon_name("view-dual-symbolic")
            .tooltip_text(gettext("Page Layout"))
            .menu_model(&view_menu)
            .build();
        header.pack_end(&layout_button);

        let zoom_menu = gio::Menu::new();
        zoom_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(Some(&gettext("Fit Width")), Some("win.fit-width"));
            m.append(Some(&gettext("Fit Page")), Some("win.fit-page"));
            m.append(Some(&gettext("Actual Size")), Some("win.zoom-actual"));
            m
        });
        zoom_menu.append_section(None, &{
            let m = gio::Menu::new();
            m.append(Some(&gettext("Zoom In")), Some("win.zoom-in"));
            m.append(Some(&gettext("Zoom Out")), Some("win.zoom-out"));
            m
        });
        let zoom_button = gtk::MenuButton::builder()
            .label("100%")
            .tooltip_text(gettext("Zoom"))
            .menu_model(&zoom_menu)
            .build();
        header.pack_end(&zoom_button);

        let page_entry = gtk::Entry::builder()
            .width_chars(4)
            .max_width_chars(6)
            .xalign(1.0)
            .input_purpose(gtk::InputPurpose::Digits)
            .tooltip_text(gettext_f(
                "Go to Page ({shortcut})",
                &[("shortcut", &primary("L"))],
            ))
            .build();
        let page_total = gtk::Label::new(None);
        page_total.add_css_class("dim-label");
        let page_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        page_box.append(&page_entry);
        page_box.append(&page_total);
        header.pack_end(&page_box);

        // Search: a bar under the header bar.
        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text(gettext("Search"))
            .hexpand(true)
            .build();
        let search_status = gtk::Label::builder()
            .css_classes(["dim-label", "numeric"])
            .width_chars(10)
            .xalign(1.0)
            .build();
        let step_buttons = gtk::Box::builder().css_classes(["linked"]).build();
        step_buttons.append(
            &gtk::Button::builder()
                .icon_name("go-up-symbolic")
                .tooltip_text(gettext("Previous Match (Shift+Enter)"))
                .action_name("win.find-previous")
                .build(),
        );
        step_buttons.append(
            &gtk::Button::builder()
                .icon_name("go-down-symbolic")
                .tooltip_text(gettext("Next Match (Enter)"))
                .action_name("win.find-next")
                .build(),
        );
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        search_row.append(&search_entry);
        search_row.append(&search_status);
        search_row.append(&step_buttons);
        let search_bar = gtk::SearchBar::builder()
            .child(
                &adw::Clamp::builder()
                    .maximum_size(560)
                    .child(&search_row)
                    .build(),
            )
            .build();
        // Not `connect_entry`: then the bar focuses the entry again once it has slid in, which
        // selects what was typed meanwhile, and the next key replaces it. The find action focuses
        // it right away.
        let search_toggle = gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text(gettext_f(
                "Search ({shortcut})",
                &[("shortcut", &primary("F"))],
            ))
            .build();
        search_bar
            .bind_property("search-mode-enabled", &search_toggle, "active")
            .bidirectional()
            .sync_create()
            .build();
        header.pack_end(&search_toggle);

        let MarkupBar {
            markup_bar,
            markup_toggle,
            color_swatch,
            signature_menu,
            text_controls,
            text_font,
            text_bold,
            text_italic,
            text_size,
        } = markup::build_bar(&header, &content);
        load_css();

        let banner = adw::Banner::builder().action_name("win.reload").build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.add_top_bar(&search_bar);
        view.add_top_bar(&banner);
        view.set_content(Some(&toasts));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(crate::APP_NAME)
            .default_width(1000)
            .default_height(800)
            .content(&view)
            .build();

        let this = Self(Rc::new(Inner {
            window,
            toasts,
            stack,
            split,
            title,
            canvas,
            page_entry,
            page_total,
            document_widgets: vec![
                sidebar_toggle.upcast(),
                search_toggle.upcast(),
                markup_toggle.upcast(),
                layout_button.upcast(),
                zoom_button.clone().upcast(),
                page_box.upcast(),
            ],
            zoom_button,
            document: RefCell::default(),
            uri: RefCell::default(),
            recent_title,
            recent_list,
            sidebar: RefCell::default(),
            sidebar_stack: RefCell::default(),
            markup_list: RefCell::default(),
            last_page: Cell::new(usize::MAX),
            last_toast: RefCell::default(),
            search_bar,
            markup_bar,
            text_controls,
            text_font,
            text_bold,
            text_italic,
            text_size,
            text_style: Cell::new((12.0, TextFont::default())),
            showing_text_style: Cell::new(false),
            color_swatch,
            page_overlay,
            signature_menu,
            placing: RefCell::default(),
            typing: RefCell::default(),
            text_editor: RefCell::default(),
            choices: RefCell::default(),
            tool_colors: Cell::new([
                "yellow", "red", "red", "red", "red", "red", "red", "black", "yellow",
            ]),
            search_entry,
            search_status,
            undo_section,
            saving: Cell::new(false),
            close_confirmed: Cell::new(false),
            alert: RefCell::default(),
            known: RefCell::default(),
            monitor: RefCell::default(),
            check_pending: Cell::new(false),
            password: RefCell::default(),
            banner,
            printed: Cell::new(0),
            properties: RefCell::default(),
        }));
        this.setup_actions();
        this.setup_page_entry();
        this.setup_search();
        this.setup_drop();
        if debug::enabled() {
            this.setup_debug_actions();
            DEBUG_WINDOWS.with_borrow_mut(|w| w.push(Rc::downgrade(&this.0)));
        }
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_view_changed(move || {
            if let Some(inner) = weak.upgrade() {
                Window(inner).view_changed();
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_picked(move || {
            if let Some(inner) = weak.upgrade() {
                let picked = inner
                    .document
                    .borrow()
                    .as_ref()
                    .and_then(|d| d.borrow().picked);
                if let Some(list) = inner.markup_list.borrow().as_ref() {
                    list.select(picked);
                }
                Window(inner).show_markup_color();
            }
        });
        this.fill_signature_menu();
        // A press on the page ends typing, before the page view makes anything of the press
        // (picking, drawing, dragging): the text field doesn't always notice by itself that it
        // lost the keyboard, as when the size field had it last.
        let press = gtk::GestureClick::new();
        press.set_button(0);
        press.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&this.0);
        press.connect_pressed(move |_, _, _, _| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).finish_typing();
            }
        });
        this.0.canvas.add_controller(press);
        // The back and forward buttons of a mouse.
        for (button, action) in [(8, "win.go-back"), (9, "win.go-forward")] {
            let click = gtk::GestureClick::new();
            click.set_button(button);
            let weak = Rc::downgrade(&this.0);
            click.connect_pressed(move |_, _, _, _| {
                if let Some(inner) = weak.upgrade() {
                    let _ = WidgetExt::activate_action(&inner.window, action, None);
                }
            });
            this.0.window.add_controller(click);
        }
        // From the size and font controls: Escape ends typing, as in the text field, and so
        // does the keyboard going elsewhere than back to it.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&this.0);
        keys.connect_key_pressed(move |_, key, _, _| {
            let typing = weak
                .upgrade()
                .filter(|inner| key == gdk::Key::Escape && inner.typing.borrow().is_some());
            match typing {
                Some(inner) => {
                    Window(inner).finish_typing();
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
        this.0.text_controls.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(&this.0);
        focus.connect_leave(move |_| {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                let Some(inner) = weak.upgrade() else { return };
                let Some(view) = inner.typing.borrow().as_ref().map(|t| t.view.clone()) else {
                    return;
                };
                let still = gtk::prelude::GtkWindowExt::focus(&inner.window).is_some_and(|f| {
                    f.is_ancestor(&inner.text_controls) || f.is_ancestor(&view) || f == view
                });
                if !still {
                    Window(inner).finish_typing();
                }
            });
        });
        this.0.text_controls.add_controller(focus);
        let weak = Rc::downgrade(&this.0);
        let chosen = move || {
            if let Some(inner) = weak.upgrade() {
                Window(inner).text_style_chosen();
            }
        };
        let f = chosen.clone();
        this.0.text_font.connect_selected_notify(move |_| f());
        let f = chosen.clone();
        this.0.text_bold.connect_toggled(move |_| f());
        let f = chosen.clone();
        this.0.text_italic.connect_toggled(move |_| f());
        this.0.text_size.connect_value_changed(move |_| chosen());
        // The text field for typing on the page goes where its text box is, as wide as the box and
        // as high as its text needs. (Left to itself, the overlay gives a text view no height at
        // all.)
        let weak = Rc::downgrade(&this.0);
        this.0
            .page_overlay
            .connect_get_child_position(move |_, widget| {
                let inner = weak.upgrade()?;
                let typing = inner.typing.borrow();
                let typing = typing.as_ref().filter(|t| t.view == *widget)?;
                let (x, y, width) = typing.area.get();
                let (_, measured, _, _) = widget.measure(gtk::Orientation::Vertical, width);
                // At least as high as the box will be for this text: the field is slow to say so
                // itself after its font has changed.
                let text = typing.text();
                let (size, font) = typing.style.get();
                let zoom = f64::from(width) / typing.place.width.max(1.0);
                let needed = if let TypingTarget::Field { height, .. } = typing.target {
                    // A field of the form is as high as it is.
                    (height * zoom).round() as i32
                } else {
                    let inside = (typing.place.width - 2.0 * specto_annot::TEXT_MARGIN).max(size);
                    let lines = specto_annot::wrap(&text, size, inside, font).len();
                    (specto_annot::box_height(lines, size, font) * zoom).ceil() as i32
                };
                let height = measured.max(needed);
                Some(gdk::Rectangle::new(x, y, width, height.max(1)))
            });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_place(move |x, y| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).place(x, y);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_fill(move |x, y| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).fill_at(x, y);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_field_key(move |key| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).field_key(key);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_edit_text(move || {
            if let Some(inner) = weak.upgrade() {
                Window(inner).edit_text(None);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_drawn(move || {
            if let Some(inner) = weak.upgrade() {
                Window(inner).annotate(|_| true);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_selected(move || {
            if let Some(inner) = weak.upgrade() {
                Window(inner).selected();
            }
        });
        // Closing the bar lets go of its tool: nothing gets marked by a bar that isn't there.
        let weak = Rc::downgrade(&this.0);
        this.0.markup_bar.connect_reveal_child_notify(move |bar| {
            if !bar.reveals_child()
                && let Some(inner) = weak.upgrade()
            {
                Window(inner).set_markup_tool(None);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.canvas.connect_open_uri(move |uri| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).open_uri(uri);
            }
        });
        this.set_document_actions_enabled(false);

        // Look at the file again when coming back to the window: file monitors miss changes on some
        // file systems, and in the Flatpak sandbox (the document portal) they miss changes made
        // outside it.
        let weak = Rc::downgrade(&this.0);
        this.0.window.connect_is_active_notify(move |window| {
            if let Some(inner) = weak.upgrade()
                && window.is_active()
            {
                Window(inner).schedule_check();
            }
        });

        // Ask about unsaved changes, and remember where the user was in this file.
        let weak = Rc::downgrade(&this.0);
        this.0.window.connect_close_request(move |_| {
            let Some(inner) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let w = Window(inner);
            // What is being typed counts as a change, and must not be lost.
            w.finish_text_entry();
            if w.is_modified() && !w.0.close_confirmed.get() {
                glib::spawn_future_local(async move { w.confirm_close().await });
                return glib::Propagation::Stop;
            }
            w.save_state();
            glib::Propagation::Proceed
        });

        this.refresh_recent();
        let weak = Rc::downgrade(&this.0);
        let handler = gtk::RecentManager::default().connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).refresh_recent();
            }
        });
        let handler = RefCell::new(Some(handler));
        this.0.window.connect_destroy(move |_| {
            if let Some(h) = handler.take() {
                gtk::RecentManager::default().disconnect(h);
            }
        });

        // Callbacks only hold weak references, so the GTK window owns the state and keeps it alive
        // until it is destroyed.
        let keep_alive = RefCell::new(Some(this.clone()));
        this.0.window.connect_destroy(move |_| {
            keep_alive.take();
        });
        this
    }

    pub fn present(&self) {
        self.0.window.present();
    }

    fn has_document(&self) -> bool {
        self.0.document.borrow().is_some()
    }

    // The file on disk

    fn set_document(&self, document: SharedDocument, tiles: async_channel::Receiver<()>) {
        let inner = &self.0;
        // A document opens with its pages whole.
        if let Some(a) = inner
            .window
            .lookup_action("trim-margins")
            .and_downcast::<gio::SimpleAction>()
        {
            a.set_state(&false.to_variant());
        }
        let name = document.borrow().name.clone();
        inner.window.set_title(Some(&name));
        inner.title.set_title(&name);

        let weak = Rc::downgrade(&self.0);
        let weak_move = Rc::downgrade(&self.0);
        let weak_files = Rc::downgrade(&self.0);
        let weak_pages = Rc::downgrade(&self.0);
        let sidebar = Sidebar::new(
            document.clone(),
            move |page| {
                if let Some(inner) = weak.upgrade() {
                    inner.canvas.update_viewer(|v| v.go_to_page(page));
                }
            },
            move |pages, to| {
                if let Some(inner) = weak_move.upgrade() {
                    Window(inner).move_pages(&pages, to);
                }
            },
            move |files, to| {
                if let Some(inner) = weak_files.upgrade() {
                    let w = Window(inner);
                    glib::spawn_future_local(async move { w.insert_files(files, Some(to)).await });
                }
            },
            move |bundle, to| {
                if let Some(inner) = weak_pages.upgrade() {
                    Window(inner).insert_bundle(&bundle, to);
                }
            },
        );
        sidebar.widget.add_controller(delete_keys());
        let stack = adw::ViewStack::new();
        stack.add_titled_with_icon(
            &sidebar.widget,
            Some("thumbnails"),
            &gettext("Thumbnails"),
            "view-grid-symbolic",
        );
        let outline = document.borrow().outline();
        if !outline.is_empty() {
            let weak = Rc::downgrade(&self.0);
            let labels = Rc::downgrade(&document);
            // Outline targets are pages of the file.
            let page_label = move |page: usize| {
                labels
                    .upgrade()
                    .and_then(|d| {
                        let d = d.borrow();
                        Some(d.page_label(d.position_of(MAIN, page)?))
                    })
                    .unwrap_or_default()
            };
            let outline = OutlineView::new(&outline, page_label, move |target| {
                if let Some(inner) = weak.upgrade() {
                    inner.canvas.follow(MAIN, target);
                }
            });
            stack.add_titled_with_icon(
                &outline.widget,
                Some("outline"),
                &gettext("Contents"),
                "view-list-symbolic",
            );
        }
        // Every annotation of the document. Filled when it is looked at: it reads the annotations
        // of every page.
        let weak = Rc::downgrade(&self.0);
        let markup = MarkupList::new(move |(position, id)| {
            if let Some(inner) = weak.upgrade() {
                inner.canvas.pick_and_show(position, id);
            }
        });
        markup.widget.add_controller(delete_keys());
        stack.add_titled_with_icon(
            &markup.widget,
            Some("markup"),
            &gettext("Markup"),
            "document-edit-symbolic",
        );
        inner.markup_list.replace(Some(markup));
        let weak = Rc::downgrade(&self.0);
        stack.connect_visible_child_name_notify(move |_| {
            if let Some(inner) = weak.upgrade() {
                Window(inner).refresh_markup_list();
            }
        });
        let switcher = adw::ViewSwitcher::builder()
            .stack(&stack)
            .policy(adw::ViewSwitcherPolicy::Narrow)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&switcher);
        column.append(&stack);
        stack.set_vexpand(true);
        inner.split.set_sidebar(Some(&column));
        inner.sidebar_stack.replace(Some(stack));
        inner.sidebar.replace(Some(sidebar));
        inner.document.replace(Some(document.clone()));
        inner.canvas.set_document(Some(document.clone()));
        inner.stack.set_visible_child_name("document");
        self.set_document_actions_enabled(true);
        inner.canvas.grab_focus();
        self.view_changed();

        // Finished tiles go into the cache, then a redraw. Not holding on to the document: once
        // it's replaced (reloaded) and dropped, its channel closes.
        let weak = Rc::downgrade(&self.0);
        let weak_doc = Rc::downgrade(&document);
        drop(document);
        glib::spawn_future_local(async move {
            while tiles.recv().await.is_ok() {
                let (Some(inner), Some(document)) = (weak.upgrade(), weak_doc.upgrade()) else {
                    break;
                };
                {
                    let mut d = document.borrow_mut();
                    if !d.receive_tiles() {
                        continue;
                    }
                    d.flush_requests();
                }
                inner.canvas.queue_draw();
                if let Some(s) = inner.sidebar.borrow().as_ref() {
                    s.refresh();
                }
            }
        });
    }

    /// Updates header bar and sidebar after the view changed.
    fn view_changed(&self) {
        self.place_typing();
        self.update_history_actions();
        self.update_trim_action();
        let inner = &self.0;
        let Some((page, zoom, mode, cover)) = inner.canvas.with_viewer(|v| {
            (
                v.current_page(),
                v.zoom(),
                v.layout_mode(),
                v.settings().cover_alone,
            )
        }) else {
            return;
        };
        inner
            .zoom_button
            .set_label(&format!("{:.0}%", zoom * 100.0));
        if inner.last_page.replace(page) != page {
            if !inner.page_entry.has_focus() {
                self.show_page_number(page);
            }
            if let Some(s) = inner.sidebar.borrow().as_ref() {
                s.follow(page);
            }
        }
        let set_state = |name: &str, state: glib::Variant| {
            if let Some(a) = inner
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                a.set_state(&state);
            }
        };
        set_state("layout", mode.name().to_variant());
        set_state("cover-alone", cover.to_variant());
    }

    /// Opens a link from the document.
    ///
    /// Only web and mail links: a PDF shouldn't be able to launch local files or other URI
    /// handlers.
    fn open_uri(&self, uri: &str) {
        let scheme = uri.split(':').next().unwrap_or("").to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https" | "mailto") {
            return self.toast(&gettext_f(
                "Links of this kind aren't opened: {address}",
                &[("address", &uri)],
            ));
        }
        let launcher = gtk::UriLauncher::new(uri);
        let this = self.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = launcher.launch_future(Some(&this.0.window)).await {
                this.toast(&gettext_f(
                    "Couldn't open link: {error}",
                    &[("error", &e.message())],
                ));
            }
        });
    }

    fn toast(&self, message: &str) {
        self.0.last_toast.replace(message.to_owned());
        self.0.toasts.add_toast(adw::Toast::new(message));
    }

    // Search

    /// Applies an edit to the document, then updates the view, the sidebar and the menus if it
    /// changed anything. `f` returns `None` if nothing changed.
    fn edit(&self, f: impl FnOnce(&mut Document) -> Option<Edited>) {
        // Text being typed is in the document before anything else is done to it: an edit to
        // the pages would move the text's place from under it, and undo would run past it.
        self.finish_text_entry();
        let inner = &self.0;
        let Some(doc) = inner.document.borrow().clone() else {
            return;
        };
        let Some(edited) = f(&mut doc.borrow_mut()) else {
            return;
        };
        if let Some(s) = inner.sidebar.borrow().as_ref() {
            s.pages_changed(&edited.select);
        }
        inner.last_page.set(usize::MAX);
        // Scrollbars, redraw, page number. In one update with going to a page, so the sidebar
        // doesn't follow a page in between.
        inner.canvas.update_viewer(|v| {
            if let Some(page) = edited.go_to {
                v.go_to_page(page);
            }
        });
        self.update_edit_state();
        self.update_search_status();
    }
}

/// How a shortcut with the primary modifier is written in tooltips: "⌘O" on macOS, "Ctrl+O"
/// elsewhere (shortcuts use GTK's `<Primary>`, which maps the same way).
fn primary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        // Translators: a keyboard shortcut as tooltips show it, the
        // Control key with another.
        gettext_f("Ctrl+{key}", &[("key", &key)])
    }
}

fn centre(v: &Viewer) -> ViewPoint {
    ViewPoint::new(v.viewport().width / 2.0, v.viewport().height / 2.0)
}

/// The app's own styles: the color swatches of the markup bar. Loaded once, however many windows
/// there are.
fn load_css() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let mut css = String::from(
            ".markup-swatch { min-width: 18px; min-height: 18px; border-radius: 9999px; \
             box-shadow: inset 0 0 0 1px alpha(currentColor, 0.25); }\n\
             .markup-bar { background-color: @headerbar_bg_color; \
             box-shadow: inset 0 -1px alpha(currentColor, 0.12); }\n",
        );
        for (class, family) in [
            ("font-sans", FontFamily::Sans),
            ("font-serif", FontFamily::Serif),
            ("font-mono", FontFamily::Mono),
        ] {
            css.push_str(&format!(
                ".{class} {{ font-family: {}; }}\n",
                font_stack(family)
            ));
        }
        css.push_str("");
        for (name, _, c) in markup_colors() {
            let byte = |v: f32| (v * 255.0).round() as u8;
            css.push_str(&format!(
                ".markup-{name} {{ background-color: rgb({}, {}, {}); }}\n",
                byte(c.r),
                byte(c.g),
                byte(c.b)
            ));
        }
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

/// What an edit changed, for [`Window::edit`].
#[derive(Default)]
struct Edited {
    /// Pages to select in the sidebar (none: the current page).
    select: Vec<usize>,
    /// The page to show.
    go_to: Option<usize>,
}

/// Delete and Backspace delete pages on the thumbnails, and annotations in the list of markup.
///
/// Not shortcuts for the whole window, which would take the keys from text fields. Not on the
/// page view either, where a key hit by mistake would delete a page: there they delete the picked
/// annotation only (see the canvas).
fn delete_keys() -> gtk::ShortcutController {
    let keys = gtk::ShortcutController::new();
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Delete|KP_Delete|BackSpace"),
        Some(gtk::NamedAction::new("win.delete-pages")),
    ));
    keys
}

#[cfg(test)]
mod tests {
    use super::{export_name, position_after_move, unescape_xattr};

    #[test]
    fn xattr_paths() {
        assert_eq!(
            unescape_xattr(r"/home/a/R\xc3\xa9sum\xc3\xa9s/x\x5cy.pdf"),
            std::path::Path::new("/home/a/Résumés/x\\y.pdf")
        );
        assert_eq!(unescape_xattr(r"/a/\xzz"), std::path::Path::new(r"/a/\xzz"));
    }

    #[test]
    fn export_names() {
        assert_eq!(export_name("Report.pdf", &[4]), "Report (page 5).pdf");
        assert_eq!(
            export_name("Report.pdf", &[1, 2, 3, 6]),
            "Report (pages 2-4, 7).pdf"
        );
        assert_eq!(export_name("a.PDF", &[0, 2, 4, 6]), "a (4 pages).pdf");
        assert_eq!(export_name("notes", &[0, 1]), "notes (pages 1-2).pdf");
    }

    /// Checks `position_after_move` against moving the pages of a list.
    #[test]
    fn pages_keep_their_place_after_a_move() {
        for (moved, to) in [(vec![0], 3), (vec![4, 1], 0), (vec![2, 3], 6), (vec![5], 2)] {
            let mut doc = specto_doc::EditDoc::from_source(specto_doc::SourceId(0), 6);
            let range = doc.move_pages(&moved, to).expect("moved");
            for page in 0..6u32 {
                let now = doc.pages().iter().position(|p| p.page == page).unwrap();
                assert_eq!(
                    position_after_move(&moved, range.start, page as usize),
                    now,
                    "page {page}, moving {moved:?} to {to}"
                );
            }
        }
    }
}
