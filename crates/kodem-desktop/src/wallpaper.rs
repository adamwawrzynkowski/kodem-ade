//! Kodem's own copy of the wallpaper behind the chat background.
//!
//! A chosen image is copied into the data directory, and the background reads that copy, so
//! the original file can move or go. The copy is named by the hash of its bytes: another image
//! gets another path, which the window's image cache has not loaded yet.
use anyhow::{Context, Result, ensure};
use futures::FutureExt as _;
use gpui::{App, ImgResourceLoader, Resource};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

/// Folder of the data directory that holds the copy.
const FOLDER: &str = "wallpaper";
/// Largest image that Kodem copies, in bytes.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Copies the image at `source` into the data directory, removes the earlier copies and returns
/// the path of the new copy. A file that Kodem cannot read as an image is refused.
pub fn cache(source: &Path) -> Result<PathBuf> {
    let size = fs::metadata(source)
        .with_context(|| format!("could not read {}", source.display()))?
        .len();
    ensure!(size <= MAX_BYTES, "the image is larger than 64 MB");
    let bytes = fs::read(source).with_context(|| format!("could not read {}", source.display()))?;
    image::ImageReader::new(io::Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()
        .context("the file is not an image in a format that Kodem reads")?;
    let folder = kodem_core::ipc::data_dir()?.join(FOLDER);
    fs::create_dir_all(&folder)?;
    let name = format!("{:x}", Sha256::digest(&bytes));
    let path = match source.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => folder.join(format!("{name}.{}", extension.to_ascii_lowercase())),
        None => folder.join(name),
    };
    if !path.exists() {
        // A copy is complete or absent: it is written beside its place and then moved there.
        let mut staged = tempfile::NamedTempFile::new_in(&folder)?;
        staged.write_all(&bytes)?;
        staged.persist(&path)?;
    }
    clear(Some(&path))?;
    Ok(path)
}

/// Removes every copy in the data directory except `keep`.
pub fn clear(keep: Option<&Path>) -> Result<()> {
    let folder = kodem_core::ipc::data_dir()?.join(FOLDER);
    let entries = match fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if keep != Some(path.as_path()) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Releases the decoded image at `path` and its textures in every window. A wallpaper that the
/// user replaced or removed does not show again.
pub fn release(path: &Path, cx: &mut App) {
    let resource = Resource::Path(path.into());
    if let Some(Ok(image)) = cx
        .fetch_asset::<ImgResourceLoader>(&resource)
        .0
        .now_or_never()
    {
        cx.drop_image(image, None);
    }
    cx.remove_asset::<ImgResourceLoader>(&resource);
}
