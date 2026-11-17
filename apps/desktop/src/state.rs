//! Per-file view state (page, position, zoom, layout, trimmed margins, sidebar), remembered across sessions in
//! `$XDG_STATE_HOME/specto/view-state.ini`.

use std::path::PathBuf;

use gtk::glib;
use specto_view::{LayoutMode, ViewState, ZoomMode};

/// Older entries beyond this many files are dropped.
const MAX_FILES: usize = 200;

/// What a window remembers about a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    pub view: ViewState,
    pub sidebar: bool,
    /// "thumbnails" or "outline".
    pub sidebar_page: String,
    pub trim_margins: bool,
}

fn path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| glib::home_dir().join(".local/state"));
    base.join("specto/view-state.ini")
}

fn load_file() -> glib::KeyFile {
    let file = glib::KeyFile::new();
    // Missing or unreadable: start fresh.
    let _ = file.load_from_file(path(), glib::KeyFileFlags::NONE);
    file
}

/// The saved state for `uri`, if any (and if it parses).
pub fn load(uri: &str) -> Option<Saved> {
    let f = load_file();
    let int = |k| f.integer(uri, k).ok();
    let dbl = |k| f.double(uri, k).ok();
    let string = |k| f.string(uri, k).ok().map(|s| s.to_string());
    Some(Saved {
        view: ViewState {
            page: usize::try_from(int("page")?).ok()?,
            page_offset: dbl("page-offset").unwrap_or(0.0),
            layout: string("layout").and_then(|s| LayoutMode::from_name(&s))?,
            cover_alone: f.boolean(uri, "cover-alone").unwrap_or(false),
            zoom_mode: string("zoom-mode").and_then(|s| ZoomMode::from_name(&s))?,
            zoom: dbl("zoom").unwrap_or(1.0),
        },
        sidebar: f.boolean(uri, "sidebar").unwrap_or(true),
        sidebar_page: string("sidebar-page").unwrap_or_else(|| "thumbnails".into()),
        trim_margins: f.boolean(uri, "trim-margins").unwrap_or(false),
    })
}

/// Saves the state for `uri`, keeping only the most recently used files.
pub fn save(uri: &str, saved: &Saved) {
    let f = load_file();
    let v = &saved.view;
    f.set_integer(uri, "page", v.page.min(i32::MAX as usize) as i32);
    f.set_double(uri, "page-offset", v.page_offset);
    f.set_string(uri, "layout", v.layout.name());
    f.set_boolean(uri, "cover-alone", v.cover_alone);
    f.set_string(uri, "zoom-mode", v.zoom_mode.name());
    f.set_double(uri, "zoom", v.zoom);
    f.set_boolean(uri, "sidebar", saved.sidebar);
    f.set_string(uri, "sidebar-page", &saved.sidebar_page);
    f.set_boolean(uri, "trim-margins", saved.trim_margins);
    let now = glib::real_time() / 1_000_000;
    f.set_int64(uri, "last-used", now);

    let mut groups: Vec<(i64, String)> = f
        .groups()
        .iter()
        .map(|g| (f.int64(g, "last-used").unwrap_or(0), g.to_string()))
        .collect();
    if groups.len() > MAX_FILES {
        groups.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        for (_, g) in &groups[MAX_FILES..] {
            let _ = f.remove_group(g);
        }
    }

    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = f.save_to_file(&path) {
        glib::g_warning!(
            "specto",
            "couldn't save view state to {}: {e}",
            path.display()
        );
    }
}
