//! What the apps keep on disk between runs: drafts of documents with unsaved edits, and the images
//! those use.
//!
//! A store is a directory with two in it:
//!
//! - `drafts`: one directory per draft, named by a digest of its key. It holds `draft.json`, with
//!   the key, the fingerprint of the file and the edits, and the files pages were inserted from,
//!   each named by a digest of its content.
//! - `images`: the images of stamps, such as a signature from a photo, as PNG files named by their
//!   digest (see `StampImage::digest`). Each is kept once, however many drafts use it.
//!
//! Every file is written beside its place and then renamed into it, so a draft is the one before
//! or the one after, never part of each. The calls of all stores of a process take turns.
//!
//! Later versions of the apps must read the drafts of earlier ones: an update ends the app, often
//! with a draft waiting. `Edits` has a variant for each format. A change to the edit model that
//! makes old drafts unreadable adds one, with the types as they were and a conversion. The test
//! `drafts_of_earlier_versions_are_read` reads a draft of each format there was.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use image::{ExtendedColorType, ImageEncoder, ImageFormat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specto_annot::{StampImage, with_image_loader};
use specto_doc::EditState;
use specto_session::Draft;
use specto_source::PdfData;

/// The format drafts are written in: the latest variant of `Edits`.
pub const FORMAT: u32 = 1;

/// The name of the file of a draft in its directory.
const DRAFT: &str = "draft.json";

/// Makes the calls of all stores of the process take turns: pruning must not delete an image a
/// draft being written is about to name.
static TURN: Mutex<()> = Mutex::new(());

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("the draft can't be read: {0}")]
    Format(#[from] serde_json::Error),
    #[error("the file changed since the draft was made")]
    Changed,
    #[error("an image can't be written: {0}")]
    Image(#[from] image::ImageError),
}

/// The edits of a draft, in each format there was.
#[derive(Serialize, Deserialize)]
enum Edits {
    V1(EditState),
}

/// A draft as written.
#[derive(Serialize, Deserialize)]
struct Stored {
    /// What names the document (see [`Store::save_draft`]).
    key: String,
    /// What the file was when the draft was made.
    fingerprint: String,
    /// The digests of the images the edits use.
    images: Vec<String>,
    /// The digests of the files pages were inserted from, by their source number from 1.
    files: Vec<String>,
    edits: Edits,
}

/// What a draft names, read without its edits, which may be of a format this version doesn't know.
#[derive(Deserialize)]
struct Head {
    key: String,
    fingerprint: String,
    images: Vec<String>,
}

/// A directory with the drafts and images of an app.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store in `root`, which is made if it isn't there.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let store = Self { root: root.into() };
        fs::create_dir_all(store.drafts())?;
        fs::create_dir_all(store.images())?;
        Ok(store)
    }

    /// Keeps `draft` as the draft of the document `key`, in place of any it had.
    ///
    /// `key` names the document, such as the path of its file. `fingerprint` says what the file
    /// is, such as its size and time of change: a draft is only for the file it was made of. Where
    /// the key says that already (a digest of the content), the two can be the same.
    pub fn save_draft(
        &self,
        key: &str,
        fingerprint: &str,
        draft: &Draft,
    ) -> Result<(), StoreError> {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.draft_dir(key);
        fs::create_dir_all(&dir)?;
        let mut images = Vec::new();
        for stamp in draft.state.stamp_images() {
            let digest = stamp.digest();
            let path = self.images().join(format!("{digest}.png"));
            if !path.exists() {
                write_atomically(&path, &png(&stamp)?)?;
            }
            images.push(digest);
        }
        let mut files = Vec::new();
        for data in &draft.files {
            let digest = digest(data);
            let path = dir.join(format!("{digest}.pdf"));
            if !path.exists() {
                write_atomically(&path, data)?;
            }
            files.push(digest);
        }
        let stored = Stored {
            key: key.to_owned(),
            fingerprint: fingerprint.to_owned(),
            images,
            files,
            edits: Edits::V1(draft.state.clone()),
        };
        write_atomically(&dir.join(DRAFT), &serde_json::to_vec(&stored)?)?;
        // Files an earlier draft had and this one doesn't.
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path.file_stem().and_then(|n| n.to_str());
            let file = path.extension().is_some_and(|e| e == "pdf");
            if file && name.is_none_or(|n| !stored.files.iter().any(|f| f == n)) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    /// The draft of the document `key`, if it has one.
    ///
    /// A draft of another file than `fingerprint` says is [`StoreError::Changed`]. Whether to
    /// delete it then is up to the caller.
    pub fn load_draft(&self, key: &str, fingerprint: &str) -> Result<Option<Draft>, StoreError> {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.draft_dir(key);
        let path = dir.join(DRAFT);
        let bytes = match fs::read(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            read => read?,
        };
        let head: Head = serde_json::from_slice(&bytes)?;
        if head.key != key {
            return Ok(None);
        }
        if head.fingerprint != fingerprint {
            return Err(StoreError::Changed);
        }
        let images = self.images();
        let stored: Stored = with_image_loader(
            move |digest| read_png(&images, digest),
            || serde_json::from_slice(&bytes),
        )?;
        let files = (stored.files.iter())
            .map(|digest| match is_digest(digest) {
                true => PdfData::open(dir.join(format!("{digest}.pdf"))),
                false => Err(io::Error::new(io::ErrorKind::InvalidData, "not a digest")),
            })
            .collect::<io::Result<Vec<_>>>()?;
        // In use again: its age counts from now.
        File::options()
            .write(true)
            .open(&path)?
            .set_modified(SystemTime::now())?;
        let Edits::V1(state) = stored.edits;
        Ok(Some(Draft { state, files }))
    }

    /// Deletes the draft of the document `key`, if it has one.
    ///
    /// Its images stay until [`prune`](Self::prune).
    pub fn delete_draft(&self, key: &str) -> io::Result<()> {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        match fs::remove_dir_all(self.draft_dir(key)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    /// Deletes the drafts that weren't written or loaded for longer than `max_age`, those that
    /// can't be read, and the images no draft names.
    pub fn prune(&self, max_age: Duration) -> io::Result<()> {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let now = SystemTime::now();
        let mut used: HashSet<String> = HashSet::new();
        for entry in fs::read_dir(self.drafts())? {
            let dir = entry?.path();
            let path = dir.join(DRAFT);
            let age = (fs::metadata(&path).and_then(|m| m.modified()).ok())
                .and_then(|m| now.duration_since(m).ok());
            let head = (fs::read(&path).ok())
                .and_then(|bytes| serde_json::from_slice::<Head>(&bytes).ok());
            match (age, head) {
                (Some(age), Some(head)) if age <= max_age => used.extend(head.images),
                _ => fs::remove_dir_all(&dir)?,
            }
        }
        for entry in fs::read_dir(self.images())? {
            let path = entry?.path();
            let name = path.file_stem().and_then(|n| n.to_str());
            if name.is_none_or(|n| !used.contains(n)) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    fn drafts(&self) -> PathBuf {
        self.root.join("drafts")
    }

    fn images(&self) -> PathBuf {
        self.root.join("images")
    }

    /// The directory of the draft of `key`, named by part of its digest: a key can be anything,
    /// a name can't.
    fn draft_dir(&self, key: &str) -> PathBuf {
        self.drafts().join(&digest(key.as_bytes())[..32])
    }
}

/// The SHA-256 digest of `bytes`, in hexadecimal.
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether `name` is a digest, which makes it safe to name a file with.
fn is_digest(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Writes `bytes` beside `path` and then renames the file into its place.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".new");
    let new = path.with_file_name(name);
    let mut file = File::create(&new)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&new, path)
}

fn png(stamp: &StampImage) -> Result<Vec<u8>, image::ImageError> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out).write_image(
        &stamp.rgba,
        stamp.width,
        stamp.height,
        ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

/// The image `digest` names in `dir`, if it is there.
fn read_png(dir: &Path, digest: &str) -> Option<StampImage> {
    if !is_digest(digest) {
        return None;
    }
    let bytes = fs::read(dir.join(format!("{digest}.png"))).ok()?;
    let rgba = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .ok()?
        .into_rgba8();
    Some(StampImage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}
