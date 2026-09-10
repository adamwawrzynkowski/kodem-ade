//! Clipboard intent and private staging for the existing host attachment importer.
use anyhow::{Context, Result, ensure};
use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};
use std::{
    io::{Cursor, Write},
    path::PathBuf,
};

#[derive(Clone)]
pub struct PastedAttachments {
    pub paths: Vec<PathBuf>,
    pub images: Vec<Image>,
}
pub enum PasteContent {
    Text(String),
    Attachments(PastedAttachments),
}

/// Prefers clipboard files and image data in the composer, retaining normal text-field paste behavior.
pub fn content(item: ClipboardItem, composer: bool) -> Option<PasteContent> {
    if composer {
        let mut paths = Vec::new();
        let mut images = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::ExternalPaths(files) => paths.extend_from_slice(files.paths()),
                ClipboardEntry::Image(image) => images.push(image.clone()),
                ClipboardEntry::String(_) => {}
            }
        }
        if !paths.is_empty() || !images.is_empty() {
            return Some(PasteContent::Attachments(PastedAttachments {
                paths,
                images,
            }));
        }
    }
    item.text().map(PasteContent::Text)
}

/// Stages clipboard images in private files that delete themselves after import or cancellation.
pub fn stage_images(images: Vec<Image>) -> Result<Vec<tempfile::TempPath>> {
    const FILE_LIMIT: usize = 32 * 1024 * 1024;
    ensure!(images.len() <= 32, "Paste at most 32 images at once");
    let mut staged = Vec::new();
    let mut total = 0;
    for image in images {
        ensure!(
            image.bytes.len() <= FILE_LIMIT,
            "Image exceeds the 32 MiB attachment limit"
        );
        let (extension, bytes) = match image.format {
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::Webp => {
                (image.format.extension(), image.bytes)
            }
            _ => {
                // macOS screenshot/Preview clipboards may supply TIFF; the provider accepts PNG.
                let decoded = image::ImageReader::new(Cursor::new(image.bytes))
                    .with_guessed_format()?
                    .decode()
                    .context("Clipboard image format could not be decoded")?;
                let mut png = Cursor::new(Vec::new());
                decoded.write_to(&mut png, image::ImageFormat::Png)?;
                ("png", png.into_inner())
            }
        };
        ensure!(
            bytes.len() <= FILE_LIMIT,
            "Converted image exceeds the 32 MiB attachment limit"
        );
        total += bytes.len();
        ensure!(total <= 128 * 1024 * 1024, "Image batch exceeds 128 MiB");
        let mut file = tempfile::Builder::new()
            .prefix("Pasted image-")
            .suffix(&format!(".{extension}"))
            .tempfile()?;
        file.write_all(&bytes)?;
        file.flush()?;
        staged.push(file.into_temp_path());
    }
    Ok(staged)
}
