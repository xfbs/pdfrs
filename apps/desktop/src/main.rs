//! Specto, the desktop app (GTK 4 and libadwaita).
//!
//! `specto-gtk [FILE]...` opens each file in its own window.

mod canvas;
mod debug;
mod document;
mod i18n;
mod markup_list;
mod outline;
mod print;
mod properties;
mod signatures;
mod state;
mod thumbnails;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

use i18n::{gettext, gettext_f};
use window::Window;

/// The application ID: D-Bus name, `.desktop` file and icon name, Flatpak ID.
///
/// The files in `data/` are named after it. Renaming the app means changing this, those file names
/// and their contents.
pub const APP_ID: &str = "net.xfbs.specto";

/// The name shown to people: window titles, the macOS menu.
pub const APP_NAME: &str = "Specto";

/// The project's home page, and where problems are reported. Also in
/// `data/net.xfbs.specto.metainfo.xml`.
const WEBSITE: &str = "https://github.com/xfbs/specto";
const ISSUES: &str = "https://github.com/xfbs/specto/issues";

/// The keys for the Properties dialog: what the platform's file manager uses.
const PROPERTIES_ACCELS: &[&str] = if cfg!(target_os = "macos") {
    &["<Meta>i"]
} else {
    &["<Alt>Return"]
};

/// The keys for going back and forward after a jump: what the platform's browsers use.
const BACK_ACCELS: &[&str] = if cfg!(target_os = "macos") {
    &["<Meta>bracketleft"]
} else {
    &["<Alt>Left"]
};
const FORWARD_ACCELS: &[&str] = if cfg!(target_os = "macos") {
    &["<Meta>bracketright"]
} else {
    &["<Alt>Right"]
};

fn main() -> glib::ExitCode {
    i18n::init();
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    glib::set_application_name(APP_NAME);
    app.connect_startup(|app| {
        gtk::Window::set_default_icon_name(APP_ID);
        for (action, accels) in [
            ("win.open", &["<Primary>o"][..]),
            ("win.save", &["<Primary>s"]),
            ("win.save-as", &["<Shift><Primary>s"]),
            ("win.print", &["<Primary>p"]),
            ("win.properties", PROPERTIES_ACCELS),
            (
                "win.zoom-in",
                &["<Primary>plus", "<Primary>equal", "<Primary>KP_Add"],
            ),
            ("win.zoom-out", &["<Primary>minus", "<Primary>KP_Subtract"]),
            ("win.zoom-actual", &["<Primary>0", "<Primary>KP_0"]),
            ("win.fit-width", &["<Primary>8"]),
            ("win.fit-page", &["<Primary>9"]),
            ("win.go-to-page", &["<Primary>l"]),
            ("win.go-back", BACK_ACCELS),
            ("win.go-forward", FORWARD_ACCELS),
            ("win.copy", &["<Primary>c"]),
            ("win.select-all", &["<Primary>a"]),
            ("win.find", &["<Primary>f"]),
            ("win.find-next", &["<Primary>g"]),
            ("win.find-previous", &["<Shift><Primary>g"]),
            ("win.undo", &["<Primary>z"]),
            ("win.redo", &["<Shift><Primary>z"]),
            ("win.show-markup", &["<Shift><Primary>a"]),
            ("win.highlight", &["<Shift><Primary>h"]),
            ("win.underline", &["<Shift><Primary>u"]),
            ("win.strike-out", &["<Shift><Primary>x"]),
            ("win.rotate-left", &["<Primary>Left"]),
            ("win.rotate-right", &["<Primary>Right"]),
            ("win.layout::single", &["<Primary>1"]),
            ("win.layout::continuous", &["<Primary>2"]),
            ("win.layout::two-page", &["<Primary>3"]),
            ("win.layout::two-page-continuous", &["<Primary>4"]),
            ("win.show-sidebar", &["F9"]),
            ("window.close", &["<Primary>w"]),
            ("app.quit", &["<Primary>q"]),
        ] {
            let accels = platform_accels(accels);
            let accels: Vec<&str> = accels.iter().map(String::as_str).collect();
            app.set_accels_for_action(action, &accels);
        }
        // macOS shows this in the system menu bar. Elsewhere the in-window header bar menus are
        // enough (and libadwaita windows don't show a menu bar anyway).
        if cfg!(target_os = "macos") {
            app.set_menubar(Some(&menubar()));
        }
        // Close windows one by one, so each saves its view state.
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &adw::Application, _, _| {
                for window in app.windows() {
                    window.close();
                }
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &adw::Application, _, _| show_about(app))
            .build();
        app.add_action_entries([quit, about]);
    });

    app.connect_activate(|app| Window::new(app).present());
    app.connect_open(|app, files, _hint| {
        for file in files {
            let window = Window::new(app);
            window.present();
            window.open(file.clone());
        }
    });
    app.run()
}

/// Shows the About dialog over the active window.
fn show_about(app: &adw::Application) {
    let licence = gettext_f(
        // Translators: {apache} and {mit} are the names of the two licenses,
        // which stay in English and become links.
        "Specto is licensed under either of the {apache} or the {mit}, at your option.\n\n\
         It comes with no warranty of any kind.",
        &[
            (
                "apache",
                &"<a href=\"https://www.apache.org/licenses/LICENSE-2.0\">Apache License, \
                  Version 2.0</a>",
            ),
            (
                "mit",
                &"<a href=\"https://opensource.org/license/mit\">MIT license</a>",
            ),
        ],
    );
    let dialog = adw::AboutDialog::builder()
        .application_name(APP_NAME)
        .application_icon(APP_ID)
        // The version of the apps, which the crates' own versions don't follow.
        .version(include_str!("../../../VERSION").trim())
        .developer_name(gettext("The Specto contributors"))
        .copyright(gettext("© 2026 The Specto contributors"))
        .website(WEBSITE)
        .issue_url(ISSUES)
        .license_type(gtk::License::Custom)
        .license(licence)
        .build();
    // The two libraries that do the heavy lifting. The text before each
    // address is shown, as a link to it.
    // Translators: what the library is used for stays in brackets.
    let hayro = gettext("hayro (rendering)");
    // Translators: what the library is used for stays in brackets.
    let lopdf = gettext("lopdf (saving)");
    dialog.add_acknowledgement_section(
        Some(&gettext("Built with")),
        &[
            &format!("{hayro} https://github.com/LaurenzV/hayro"),
            &format!("{lopdf} https://github.com/J-F-Liu/lopdf"),
        ],
    );
    dialog.present(app.active_window().as_ref());
}

/// On macOS, registers each `<Primary>` shortcut with `<Meta>` (Cmd) first: GTK's macOS backend
/// maps `<Primary>` to Ctrl, and menus display the first shortcut of an action, so Cmd must come
/// first to show as "⌘O".
///
/// The Ctrl version stays as a second, undisplayed shortcut.
fn platform_accels(accels: &[&str]) -> Vec<String> {
    let primary: Vec<String> = accels.iter().map(|a| a.to_string()).collect();
    if !cfg!(target_os = "macos") {
        return primary;
    }
    let mut out: Vec<String> = accels
        .iter()
        .filter(|a| a.contains("<Primary>"))
        .map(|a| a.replace("<Primary>", "<Meta>"))
        .collect();
    out.extend(primary);
    out
}

/// The application menu bar (macOS only): File, Edit, View, Go, Tools and Help, like Preview.
fn menubar() -> gio::Menu {
    let quit = gettext_f("Quit {app}", &[("app", &APP_NAME)]);
    let about = gettext_f("About {app}", &[("app", &APP_NAME)]);
    let section = |items: &[(&str, &str)]| {
        let m = gio::Menu::new();
        for (label, action) in items {
            m.append(Some(label), Some(action));
        }
        m
    };
    let submenu = |sections: &[gio::Menu]| {
        let m = gio::Menu::new();
        for s in sections {
            m.append_section(None, s);
        }
        m
    };

    let file = submenu(&[
        section(&[(&gettext("Open…"), "win.open")]),
        section(&[
            (&gettext("Save"), "win.save"),
            (&gettext("Save As…"), "win.save-as"),
            (&gettext("Export Selected Pages…"), "win.export-pages"),
        ]),
        section(&[
            (&gettext("Print…"), "win.print"),
            (&gettext("Properties"), "win.properties"),
        ]),
        section(&[(&gettext("Close Window"), "window.close")]),
        section(&[(&quit, "app.quit")]),
    ]);
    let edit = submenu(&[
        section(&[
            (&gettext("Undo"), "win.undo"),
            (&gettext("Redo"), "win.redo"),
        ]),
        section(&[
            (&gettext("Copy"), "win.copy"),
            (&gettext("Select All"), "win.select-all"),
        ]),
        section(&[
            (&gettext("Insert Pages from File…"), "win.insert-file"),
            (&gettext("Delete Pages"), "win.delete-pages"),
        ]),
        section(&[
            (&gettext("Find…"), "win.find"),
            (&gettext("Find Next"), "win.find-next"),
            (&gettext("Find Previous"), "win.find-previous"),
        ]),
    ]);
    let view = submenu(&[
        section(&[
            (&gettext("Single Page"), "win.layout::single"),
            (&gettext("Continuous"), "win.layout::continuous"),
            (&gettext("Two Pages"), "win.layout::two-page"),
            (
                &gettext("Two Pages, Continuous"),
                "win.layout::two-page-continuous",
            ),
        ]),
        section(&[
            (&gettext("Show Cover Page Alone"), "win.cover-alone"),
            (&gettext("Trim Margins"), "win.trim-margins"),
        ]),
        section(&[
            (&gettext("Actual Size"), "win.zoom-actual"),
            (&gettext("Fit Width"), "win.fit-width"),
            (&gettext("Fit Page"), "win.fit-page"),
            (&gettext("Zoom In"), "win.zoom-in"),
            (&gettext("Zoom Out"), "win.zoom-out"),
        ]),
        section(&[
            (&gettext("Show Sidebar"), "win.show-sidebar"),
            (&gettext("Thumbnails"), "win.sidebar-page::thumbnails"),
            (&gettext("Contents"), "win.sidebar-page::outline"),
            (&gettext("Markup"), "win.sidebar-page::markup"),
        ]),
    ]);
    let go = submenu(&[
        section(&[
            (&gettext("Previous Page"), "win.previous-page"),
            (&gettext("Next Page"), "win.next-page"),
        ]),
        section(&[
            (&gettext("First Page"), "win.first-page"),
            (&gettext("Last Page"), "win.last-page"),
        ]),
        section(&[(&gettext("Go to Page…"), "win.go-to-page")]),
        section(&[
            (&gettext("Back"), "win.go-back"),
            (&gettext("Forward"), "win.go-forward"),
        ]),
    ]);

    let tools = submenu(&[
        section(&[
            (&gettext("Rotate Left"), "win.rotate-left"),
            (&gettext("Rotate Right"), "win.rotate-right"),
        ]),
        section(&[
            (&gettext("Show Markup Toolbar"), "win.show-markup"),
            (&gettext("Highlight"), "win.highlight"),
            (&gettext("Underline"), "win.underline"),
            (&gettext("Strikethrough"), "win.strike-out"),
        ]),
        section(&[
            (&gettext("Highlight Form Fields"), "win.highlight-fields"),
            (&gettext("Reset Form"), "win.reset-form"),
        ]),
    ]);

    let bar = gio::Menu::new();
    bar.append_submenu(Some(&gettext("File")), &file);
    bar.append_submenu(Some(&gettext("Edit")), &edit);
    bar.append_submenu(Some(&gettext("View")), &view);
    bar.append_submenu(Some(&gettext("Go")), &go);
    bar.append_submenu(Some(&gettext("Tools")), &tools);
    bar.append_submenu(
        Some(&gettext("Help")),
        &submenu(&[section(&[(&about, "app.about")])]),
    );
    bar
}
