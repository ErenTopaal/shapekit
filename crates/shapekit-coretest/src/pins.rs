//! The test image, read from `pins.toml` by digest. docker is never given the tag.

use std::path::Path;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Image {
    pub repository: String,
    pub tag: String,
    pub digest: String,
}

#[derive(serde::Deserialize)]
struct Pins {
    #[serde(default)]
    image: Vec<Entry>,
}

#[derive(serde::Deserialize)]
struct Entry {
    name: String,
    repository: String,
    tag: String,
    digest: String,
}

impl Image {
    /// Reads the image entry called `name` from a `pins.toml`.
    pub fn from_pins(path: &Path, name: &str) -> Result<Image> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
            path: shown.clone(),
            source,
        })?;
        Self::from_pins_text(&text, &shown, name)
    }

    pub fn from_pins_text(text: &str, shown: &str, name: &str) -> Result<Image> {
        let pins: Pins = toml::from_str(text).map_err(|source| Error::Pins {
            path: shown.to_owned(),
            source,
        })?;
        let entry = pins
            .image
            .into_iter()
            .find(|i| i.name == name)
            .ok_or_else(|| Error::MissingImage {
                path: shown.to_owned(),
                name: name.to_owned(),
            })?;
        let hex_part = entry.digest.strip_prefix("sha256:").unwrap_or("");
        if hex_part.len() != 64 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::BadDigest {
                name: entry.name,
                digest: entry.digest,
            });
        }
        Ok(Image {
            repository: entry.repository,
            tag: entry.tag,
            digest: entry.digest,
        })
    }

    /// `repository@sha256:...`.
    pub fn reference(&self) -> String {
        format!("{}@{}", self.repository, self.digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_committed_test_image() {
        let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
        let image = Image::from_pins(&pins, crate::BUILDTESTS_IMAGE_PIN).unwrap();
        assert_eq!(image.repository, "stellar/unsafe-stellar-core");
        assert!(image.reference().contains("@sha256:"));
    }

    #[test]
    fn rejects_a_tag_in_place_of_a_digest() {
        let text =
            "[[image]]\nname = \"x\"\nrepository = \"r\"\ntag = \"t\"\ndigest = \"latest\"\n";
        let err = Image::from_pins_text(text, "p", "x").unwrap_err();
        assert!(matches!(err, Error::BadDigest { .. }));
    }

    #[test]
    fn reports_a_missing_entry() {
        let err = Image::from_pins_text("schema = 1\n", "p", "x").unwrap_err();
        assert!(matches!(err, Error::MissingImage { .. }));
    }
}
