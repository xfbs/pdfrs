//! Files: opening, watching for changes on disk, saving, exporting, printing, and what is
//! remembered about them.

use super::*;

pub(super) fn couldnt_read(name: &str, error: &dyn std::fmt::Display) -> String {
    gettext_f(
        "Couldn't read “{name}”: {error}",
        &[("name", &name), ("error", error)],
    )
}

pub(super) fn couldnt_save(file: &gio::File, error: &dyn std::fmt::Display) -> String {
    gettext_f(
        "Couldn't save “{name}”: {error}",
        &[("name", &file_name(file)), ("error", error)],
    )
}

/// A file dialog for choosing a PDF.
pub(super) fn pdf_dialog(title: &str) -> gtk::FileDialog {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("PDF documents")));
    filter.add_mime_type("application/pdf");
    filter.add_suffix("pdf");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    gtk::FileDialog::builder()
        .title(title)
        .filters(&filters)
        .default_filter(&filter)
        .build()
}

/// The name of `file` to show ("Report.pdf").
pub(super) async fn display_name(file: &gio::File) -> String {
    file.query_info_future(
        gio::FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME,
        gio::FileQueryInfoFlags::NONE,
        glib::Priority::DEFAULT,
    )
    .await
    .map(|i| i.display_name().to_string())
    .unwrap_or_else(|_| file.parse_name().to_string())
}

/// Reads `file`, mapping it if it's large and local (see [`PdfData`]).
///
/// Files without a local path (remote locations) are read through GIO.
pub(super) async fn read_pdf(file: &gio::File) -> Result<PdfData, String> {
    if let Some(path) = file.path() {
        return gio::spawn_blocking(move || PdfData::open(path))
            .await
            .map_err(|_| gettext("reading failed"))?
            .map_err(|e| {
                // "No such file or directory (os error 2)", without the code.
                let text = e.to_string();
                text.split(" (os error").next().unwrap_or(&text).to_owned()
            });
    }
    let (bytes, _) = file
        .load_contents_future()
        .await
        .map_err(|e| e.message().to_owned())?;
    Ok(bytes.to_vec().into())
}

/// What tells whether a file changed: its entity tag (for local files, the modification time to the
/// microsecond) and size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Fingerprint {
    pub(super) etag: String,
    pub(super) size: i64,
}

/// `file`'s fingerprint, or `None` if it isn't there.
pub(super) async fn fingerprint(file: &gio::File) -> Option<Fingerprint> {
    let info = file
        .query_info_future(
            "etag::value,standard::size",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await
        .ok()?;
    Some(Fingerprint {
        etag: info.etag().map(|e| e.to_string()).unwrap_or_default(),
        size: info.size(),
    })
}

/// Writes `job` to `file` in the background: through a temporary file renamed into place if it's
/// local, else through GIO.
pub(super) async fn write_job(job: SaveJob, file: &gio::File) -> Result<(), String> {
    match file.path() {
        Some(path) => gio::spawn_blocking(move || job.save(&path).map_err(save_error))
            .await
            .unwrap_or_else(|_| Err(gettext("saving failed"))),
        None => {
            let bytes = gio::spawn_blocking(move || job.write().map_err(save_error))
                .await
                .unwrap_or_else(|_| Err(gettext("saving failed")))?;
            file.replace_contents_future(bytes, None, false, gio::FileCreateFlags::NONE)
                .await
                .map(|_| ())
                .map_err(|(_, e)| e.message().to_owned())
        }
    }
}

/// The folder `file` is in, as people know it.
///
/// In the Flatpak sandbox, files come through the document portal, at a path under `/run/user`. The
/// portal says where they really are.
pub(super) fn folder(file: &gio::File) -> Option<gio::File> {
    const HOST_PATH: &str = "xattr::document-portal.host-path";
    let host = file
        .path()
        .filter(|p| p.starts_with("/run/user") || p.starts_with("/run/flatpak"))
        .and_then(|_| {
            file.query_info(
                HOST_PATH,
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .ok()
        })
        .and_then(|info| info.attribute_string(HOST_PATH))
        .map(|p| gio::File::for_path(unescape_xattr(&p)));
    host.unwrap_or_else(|| file.clone()).parent()
}

/// Undoes GIO's escaping of extended attributes, which writes bytes that aren't printable ASCII,
/// and backslashes, as `\xNN`.
pub(super) fn unescape_xattr(text: &str) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i..i + 4)
            .filter(|b| b.starts_with(b"\\x"))
            .and_then(|b| std::str::from_utf8(&b[2..]).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(b) => {
                out.push(b);
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    std::ffi::OsString::from_vec(out).into()
}

/// The last part of `file`'s name, for messages.
pub(super) fn file_name(file: &gio::File) -> String {
    file.basename()
        .map_or_else(String::new, |b| b.to_string_lossy().into_owned())
}

/// Why saving failed, for a message.
pub(super) fn save_error(e: specto_write::WriteError) -> String {
    use specto_write::WriteError as E;
    match e {
        E::Io(e) => {
            // "Permission denied (os error 13)", without the code.
            let text = e.to_string();
            text.split(" (os error").next().unwrap_or(&text).to_owned()
        }
        E::PasswordRequired => gettext("the document's password is needed"),
        _ => gettext_f(
            "{app} can't save this document correctly yet",
            &[("app", &crate::APP_NAME)],
        ),
    }
}

impl Window {
    /// Opens `file` here, or in a new window if this one already shows a document.
    pub fn open(&self, file: gio::File) {
        if self.has_document() {
            let app = self
                .0
                .window
                .application()
                .and_downcast::<adw::Application>();
            if let Some(app) = app {
                let w = Window::new(&app);
                w.present();
                w.open(file);
            }
            return;
        }
        let this = self.clone();
        glib::spawn_future_local(async move { this.load(file).await });
    }

    pub(super) async fn load(&self, file: gio::File) {
        let name = display_name(&file).await;
        // Before reading: a change made meanwhile then shows as one.
        let known = fingerprint(&file).await;
        let data = match read_pdf(&file).await {
            Ok(data) => data,
            Err(e) => return self.toast(&couldnt_read(&name, &e)),
        };
        let Some((document, tiles, password)) = self.open_document(&name, data, None).await else {
            return;
        };
        self.set_document(Rc::new(RefCell::new(document)), tiles);
        let uri = file.uri().to_string();
        self.restore_state(&uri);
        gtk::RecentManager::default().add_item(&uri);
        self.0.uri.replace(Some(uri));
        self.0.password.replace(password);
        self.watch(&file, known);
    }

    /// Opens `data` as a document, asking for a password if it needs one (`password` is tried
    /// first).
    ///
    /// Returns the document, its tile notifications and the password that worked, or `None` if it
    /// couldn't be opened (with a message) or the password was canceled.
    pub(super) async fn open_document(
        &self,
        name: &str,
        data: PdfData,
        mut password: Option<String>,
    ) -> Option<(Document, async_channel::Receiver<()>, Option<String>)> {
        let (tx, rx) = async_channel::unbounded::<()>();
        let mut asked = false;
        loop {
            let tx = tx.clone();
            let wake = move || {
                let _ = tx.try_send(());
            };
            match Document::open(name.to_owned(), data.clone(), password.as_deref(), wake) {
                Ok(d) => return Some((d, rx, password)),
                Err(OpenError::PasswordRequired) => {
                    password = Some(self.ask_password(name, asked).await?);
                    asked = true;
                }
                Err(e) => {
                    self.toast(&gettext_f(
                        "Couldn't open “{name}”: {error}",
                        &[("name", &name), ("error", &e)],
                    ));
                    return None;
                }
            }
        }
    }

    /// Starts watching `file`, as it was when read (`known`), for changes by other programs.
    pub(super) fn watch(&self, file: &gio::File, known: Option<Fingerprint>) {
        let inner = &self.0;
        inner.known.replace(known);
        inner.banner.set_revealed(false);
        let monitor = file
            .monitor_file(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            .ok();
        if let Some(m) = &monitor {
            let weak = Rc::downgrade(&self.0);
            m.connect_changed(move |_, _, _, _| {
                if let Some(inner) = weak.upgrade() {
                    Window(inner).schedule_check();
                }
            });
        }
        if let Some(old) = inner.monitor.replace(monitor) {
            old.cancel();
        }
    }

    /// Looks at the file shortly: a save by another program comes as several events.
    pub(super) fn schedule_check(&self) {
        if self.0.check_pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            if let Some(inner) = weak.upgrade() {
                let w = Window(inner);
                glib::spawn_future_local(async move { w.check_disk().await });
            }
        });
    }

    /// Compares the file with what was read or saved.
    ///
    /// Unchanged, nothing happens. Changed, it's reloaded, or with edits a banner offers to. Gone,
    /// a banner says so.
    pub(super) async fn check_disk(&self) {
        let inner = &self.0;
        inner.check_pending.set(false);
        if inner.saving.get() {
            // Our own save: looked at once it's done.
            return self.schedule_check();
        }
        let (Some(uri), Some(name)) = (
            inner.uri.borrow().clone(),
            inner
                .document
                .borrow()
                .as_ref()
                .map(|d| d.borrow().name.clone()),
        ) else {
            return;
        };
        let now = fingerprint(&gio::File::for_uri(&uri)).await;
        if now == *inner.known.borrow() {
            inner.banner.set_revealed(false);
            return;
        }
        let banner = &inner.banner;
        match now {
            None => {
                banner.set_title(&gettext_f(
                    "“{name}” was moved or deleted. Saving writes it again.",
                    &[("name", &name)],
                ));
                banner.set_button_label(None);
                banner.set_revealed(true);
            }
            Some(_) if !self.is_modified() => self.reload().await,
            Some(_) => {
                banner.set_title(&gettext_f(
                    "“{name}” was changed by another program. Reloading discards your edits.",
                    &[("name", &name)],
                ));
                banner.set_button_label(Some(&gettext("_Reload")));
                banner.set_revealed(true);
            }
        }
    }

    /// Whether the file changed on disk since it was read or saved (and is still there).
    pub(super) async fn changed_on_disk(&self) -> bool {
        let Some(uri) = self.0.uri.borrow().clone() else {
            return false;
        };
        let now = fingerprint(&gio::File::for_uri(&uri)).await;
        now.is_some() && now != *self.0.known.borrow()
    }

    /// Reads the file again and shows it in place of the document, edits discarded, at the same
    /// place.
    pub(super) async fn reload(&self) {
        self.finish_text_entry();
        let inner = &self.0;
        let (Some(uri), Some(doc)) = (inner.uri.borrow().clone(), inner.document.borrow().clone())
        else {
            return;
        };
        let file = gio::File::for_uri(&uri);
        let name = doc.borrow().name.clone();
        let known = fingerprint(&file).await;
        let data = match read_pdf(&file).await {
            Ok(data) => data,
            Err(e) => return self.toast(&couldnt_read(&name, &e)),
        };
        let password = inner.password.borrow().clone();
        let Some((document, tiles, password)) = self.open_document(&name, data, password).await
        else {
            return;
        };
        // The window may have moved on meanwhile.
        if !inner
            .document
            .borrow()
            .as_ref()
            .is_some_and(|d| Rc::ptr_eq(d, &doc))
        {
            return;
        }
        let view = doc.borrow().viewer.view_state();
        let trim_margins = doc.borrow().trim_margins();
        let sidebar_page = inner
            .sidebar_stack
            .borrow()
            .as_ref()
            .and_then(|s| s.visible_child_name());
        drop(doc);
        self.set_document(Rc::new(RefCell::new(document)), tiles);
        inner.canvas.update_viewer(|v| v.restore_view_state(&view));
        self.set_trim_margins(trim_margins);
        if let Some(page) = sidebar_page
            && inner.sidebar_stack.borrow().is_some()
        {
            gio::prelude::ActionGroupExt::activate_action(
                &inner.window,
                "sidebar-page",
                Some(&page.to_variant()),
            );
        }
        inner.password.replace(password);
        inner.known.replace(known);
        inner.banner.set_revealed(false);
        self.update_edit_state();
        if inner.search_bar.is_search_mode() {
            self.run_search();
        } else {
            self.update_search_status();
        }
    }

    /// Asks whether to save over a file another program changed. Returns whether to go ahead.
    pub(super) async fn confirm_overwrite(&self, name: &str) -> bool {
        let inner = &self.0;
        let alert = adw::AlertDialog::new(
            Some(&gettext("File Changed on Disk")),
            Some(&gettext_f(
                "“{name}” was changed by another program after it was opened. Saving replaces those changes with this document.",
                &[("name", &name)],
            )),
        );
        alert.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("save", &gettext("_Save Anyway")),
        ]);
        alert.set_response_appearance("save", adw::ResponseAppearance::Destructive);
        alert.set_default_response(Some("cancel"));
        alert.set_close_response("cancel");
        inner.alert.replace(Some(alert.clone()));
        let response = alert.choose_future(Some(&inner.window)).await;
        inner.alert.replace(None);
        response == "save"
    }

    /// Applies the view state saved for `uri`, if any.
    pub(super) fn restore_state(&self, uri: &str) {
        let Some(saved) = state::load(uri) else {
            return;
        };
        let inner = &self.0;
        inner
            .canvas
            .update_viewer(|v| v.restore_view_state(&saved.view));
        if inner.sidebar_stack.borrow().is_some() {
            gio::prelude::ActionGroupExt::activate_action(
                &inner.window,
                "sidebar-page",
                Some(&saved.sidebar_page.to_variant()),
            );
        }
        inner.split.set_show_sidebar(saved.sidebar);
        self.set_trim_margins(saved.trim_margins);
    }

    pub(super) fn save_state(&self) {
        let inner = &self.0;
        let (Some(uri), Some(doc)) = (inner.uri.borrow().clone(), inner.document.borrow().clone())
        else {
            return;
        };
        let view = doc.borrow().viewer.view_state();
        let trim_margins = doc.borrow().trim_margins();
        let sidebar_page = inner
            .sidebar_stack
            .borrow()
            .as_ref()
            .and_then(|s| s.visible_child_name())
            .map_or_else(|| "thumbnails".to_owned(), |n| n.to_string());
        state::save(
            &uri,
            &state::Saved {
                view,
                sidebar: inner.split.shows_sidebar(),
                sidebar_page,
                trim_margins,
            },
        );
    }

    /// Lists recently opened PDFs on the start page.
    pub(super) fn refresh_recent(&self) {
        let inner = &self.0;
        while let Some(row) = inner.recent_list.first_child() {
            inner.recent_list.remove(&row);
        }
        let mut items: Vec<gtk::RecentInfo> = gtk::RecentManager::default()
            .items()
            .into_iter()
            .filter(|i| i.mime_type() == "application/pdf" && i.exists())
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.modified().to_unix()));
        for info in items.into_iter().take(8) {
            let file = gio::File::for_uri(&info.uri());
            let subtitle = folder(&file)
                .and_then(|p| p.path())
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&info.display_name()))
                .subtitle(glib::markup_escape_text(&subtitle))
                .activatable(true)
                .build();
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let weak = Rc::downgrade(&self.0);
            row.connect_activated(move |_| {
                if let Some(inner) = weak.upgrade() {
                    Window(inner).open(file.clone());
                }
            });
            inner.recent_list.append(&row);
        }
        let any = inner.recent_list.first_child().is_some();
        inner.recent_list.set_visible(any);
        inner.recent_title.set_visible(any);
    }

    pub(super) async fn ask_password(&self, name: &str, retry: bool) -> Option<String> {
        let body = if retry {
            gettext_f(
                "The password was incorrect. Enter the password for “{name}”.",
                &[("name", &name)],
            )
        } else {
            gettext_f("“{name}” is protected with a password.", &[("name", &name)])
        };
        let dialog = adw::AlertDialog::new(Some(&gettext("Password Required")), Some(&body));
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("unlock", &gettext("_Unlock")),
        ]);
        dialog.set_response_appearance("unlock", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("unlock"));
        dialog.set_close_response("cancel");
        let entry = gtk::PasswordEntry::builder()
            .show_peek_icon(true)
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&entry));
        let response = dialog.choose_future(Some(&self.0.window)).await;
        (response == "unlock").then(|| entry.text().to_string())
    }

    /// Shows the Properties dialog for the document as it is now.
    pub(super) fn show_properties(&self) {
        let inner = &self.0;
        let Some(doc) = inner.document.borrow().clone() else {
            return;
        };
        let rows = {
            let d = doc.borrow();
            let source = d.main();
            let page_sizes = (d.viewer.pages().iter())
                .map(|page| {
                    let size = page.displayed().size();
                    (size.width, size.height)
                })
                .collect();
            properties::rows(&properties::Facts {
                name: d.name.clone(),
                folder: (inner.uri.borrow().as_ref())
                    .and_then(|u| folder(&gio::File::for_uri(u)))
                    .map(|f| f.parse_name().to_string()),
                bytes: source.pdf().data().as_ref().len() as u64,
                metadata: source.metadata(),
                version: source.version(),
                page_sizes,
                permissions: source.permissions(),
                password: inner
                    .password
                    .borrow()
                    .as_ref()
                    .is_some_and(|p| !p.is_empty()),
            })
        };
        let dialog = properties::show(&inner.window, &rows);
        inner.properties.replace(rows);
        let weak = Rc::downgrade(inner);
        dialog.connect_closed(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.properties.take();
            }
        });
    }

    /// Shows the print dialog for the document as edited (or prints to a PDF file without it: see
    /// `print::print`).
    pub(super) fn print(&self, export: Option<(std::path::PathBuf, bool)>) {
        self.finish_text_entry();
        let inner = &self.0;
        let Some(doc) = inner.document.borrow().clone() else {
            return;
        };
        let job = {
            let d = doc.borrow();
            print::Job {
                pages: d.bundle(&(0..d.edit().page_count()).collect::<Vec<_>>()),
                name: d.name.clone(),
                current: d.viewer.current_page(),
                selected: inner
                    .sidebar
                    .borrow()
                    .as_ref()
                    .map(Sidebar::selected)
                    .unwrap_or_default(),
            }
        };
        let weak = Rc::downgrade(&self.0);
        print::print(job, &inner.window, export, move |outcome| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let w = Window(inner);
            match outcome {
                print::Outcome::Done => w.0.printed.set(w.0.printed.get() + 1),
                print::Outcome::Cancelled => {}
                print::Outcome::Failed(e) => {
                    w.toast(&gettext_f("Couldn't print: {error}", &[("error", &e)]))
                }
            }
        });
    }

    pub(super) fn is_modified(&self) -> bool {
        self.0
            .document
            .borrow()
            .as_ref()
            .is_some_and(|d| d.borrow().edit().is_modified())
    }

    /// Saves the document to its file, if it has changes. Returns whether it's saved now.
    pub(super) async fn save(&self) -> bool {
        // With what is being typed, which isn't in the document until typing ends.
        self.finish_text_entry();
        let Some(uri) = self.0.uri.borrow().clone() else {
            return false;
        };
        let file = gio::File::for_uri(&uri);
        // Unchanged, unless the file is gone: then it's written again.
        if !self.is_modified() && fingerprint(&file).await.is_some() {
            return true;
        }
        if self.changed_on_disk().await && !self.confirm_overwrite(&file_name(&file)).await {
            return false;
        }
        self.save_to(file).await
    }

    /// Asks where to save a copy, saves it there, and from then on shows that file.
    pub(super) async fn save_as(&self) {
        self.finish_text_entry();
        let inner = &self.0;
        let Some(name) = inner
            .document
            .borrow()
            .as_ref()
            .map(|d| d.borrow().name.clone())
        else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save As"))
            .initial_name(&name)
            .build();
        if let Some(folder) = inner
            .uri
            .borrow()
            .as_ref()
            .and_then(|u| folder(&gio::File::for_uri(u)))
        {
            dialog.set_initial_folder(Some(&folder));
        }
        if let Ok(file) = dialog.save_future(Some(&inner.window)).await {
            self.save_as_file(file).await;
        }
    }

    /// Saves to `file` and makes it the document's file.
    pub(super) async fn save_as_file(&self, file: gio::File) -> bool {
        if !self.save_to(file.clone()).await {
            return false;
        }
        let inner = &self.0;
        let name = file.basename().map_or_else(
            || file.uri().to_string(),
            |b| b.to_string_lossy().into_owned(),
        );
        if let Some(doc) = inner.document.borrow().as_ref() {
            doc.borrow_mut().name = name.clone();
        }
        inner.window.set_title(Some(&name));
        inner.title.set_title(&name);
        let uri = file.uri().to_string();
        inner.uri.replace(Some(uri.clone()));
        gtk::RecentManager::default().add_item(&uri);
        self.save_state();
        let known = fingerprint(&file).await;
        self.watch(&file, known);
        true
    }

    /// Writes the document to `file`.
    ///
    /// Local files are written next to the old one and renamed into place (see
    /// `specto_write::replace_file`), others through GIO. Returns whether it worked, and says why
    /// if not.
    pub(super) async fn save_to(&self, file: gio::File) -> bool {
        let inner = &self.0;
        let Some(doc) = inner.document.borrow().clone() else {
            return false;
        };
        if inner.saving.replace(true) {
            return false;
        }
        let job = doc.borrow().save_job();
        let version = job.version;
        let result = write_job(job, &file).await;
        if result.is_ok() {
            // What's on disk now is ours.
            inner.known.replace(fingerprint(&file).await);
            inner.banner.set_revealed(false);
        }
        inner.saving.set(false);
        match result {
            Ok(()) => {
                doc.borrow_mut().mark_saved(version);
                self.update_edit_state();
                true
            }
            Err(e) => {
                self.toast(&couldnt_save(&file, &e));
                false
            }
        }
    }

    /// Asks whether to save changes before closing, then closes (or not).
    pub(super) async fn confirm_close(&self) {
        let inner = &self.0;
        if inner.alert.borrow().is_some() {
            return;
        }
        let name = inner
            .document
            .borrow()
            .as_ref()
            .map(|d| d.borrow().name.clone())
            .unwrap_or_default();
        let alert = adw::AlertDialog::new(
            Some(&gettext("Save Changes?")),
            Some(&gettext_f(
                "“{name}” has unsaved changes. Changes that aren't saved will be lost.",
                &[("name", &name)],
            )),
        );
        alert.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("discard", &gettext("_Discard")),
            ("save", &gettext("_Save")),
        ]);
        alert.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        alert.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some("save"));
        alert.set_close_response("cancel");
        inner.alert.replace(Some(alert.clone()));
        let response = alert.choose_future(Some(&inner.window)).await;
        inner.alert.replace(None);
        let close = match response.as_str() {
            "discard" => true,
            "save" => self.save().await,
            _ => false,
        };
        if close {
            inner.close_confirmed.set(true);
            inner.window.close();
        }
    }

    pub(super) fn choose_file(&self) {
        let dialog = pdf_dialog(&gettext("Open PDF"));
        let this = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(file) = dialog.open_future(Some(&this.0.window)).await {
                this.open(file);
            }
        });
    }
}
