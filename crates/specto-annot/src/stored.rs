//! Stamp images in stored state, such as a draft of a document.
//!
//! An image is written as its size and a digest of its pixels, never the pixels themselves: those
//! are kept once, by digest, wherever the store keeps images, however many drafts use them. To read
//! state back, the store hands a loader to [`with_image_loader`], which finds the pixels for each
//! digest. State with images can't be read without one.

use std::cell::RefCell;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::StampImage;

/// Finds the image with a digest, for reading stored state.
type Loader = Box<dyn FnMut(&str) -> Option<StampImage>>;

thread_local! {
    static LOADER: RefCell<Option<Loader>> = const { RefCell::new(None) };
}

/// Runs `read`, which reads stored state, with `load` finding the image for each digest in it.
pub fn with_image_loader<R>(
    load: impl FnMut(&str) -> Option<StampImage> + 'static,
    read: impl FnOnce() -> R,
) -> R {
    let previous = LOADER.with(|l| l.borrow_mut().replace(Box::new(load)));
    let result = read();
    LOADER.with(|l| *l.borrow_mut() = previous);
    result
}

impl StampImage {
    /// The SHA-256 digest of the image's size and pixels, in hexadecimal: its name in a store.
    pub fn digest(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.width.to_le_bytes());
        hash.update(self.height.to_le_bytes());
        hash.update(&self.rgba);
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// How an image is written: what names it, not its pixels.
#[derive(Serialize, Deserialize)]
struct Stored {
    width: u32,
    height: u32,
    digest: String,
}

impl Serialize for StampImage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Stored {
            width: self.width,
            height: self.height,
            digest: self.digest(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StampImage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = Stored::deserialize(deserializer)?;
        let image = LOADER.with(|l| {
            l.borrow_mut()
                .as_mut()
                .and_then(|load| load(&stored.digest))
        });
        match image {
            Some(image) if (image.width, image.height) == (stored.width, stored.height) => {
                Ok(image)
            }
            Some(_) => Err(D::Error::custom(format!(
                "the image {} has another size",
                stored.digest
            ))),
            None => Err(D::Error::custom(format!(
                "the image {} is missing",
                stored.digest
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;

    fn image() -> StampImage {
        StampImage {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }
    }

    #[test]
    fn an_image_is_stored_by_its_digest() {
        let image = Arc::new(image());
        let json = serde_json::to_string(&image).unwrap();
        assert!(!json.contains("rgba"));
        assert!(json.contains(&image.digest()));

        let kept = HashMap::from([(image.digest(), (*image).clone())]);
        let read: Arc<StampImage> = with_image_loader(
            move |d| kept.get(d).cloned(),
            || serde_json::from_str(&json),
        )
        .unwrap();
        assert_eq!(read, image);
    }

    #[test]
    fn a_missing_image_is_an_error() {
        let json = serde_json::to_string(&image()).unwrap();
        let read = with_image_loader(|_| None, || serde_json::from_str::<StampImage>(&json));
        assert!(read.is_err());
        assert!(serde_json::from_str::<StampImage>(&json).is_err());
    }
}
