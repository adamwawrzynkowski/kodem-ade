//! Bounded imports of project images into private, immutable icon files.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{Cursor, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// Decodes a selected image and saves a small PNG independent of the source file.
pub(super) fn import(root: &Path, source: &Path) -> Result<PathBuf> {
    const FILE_LIMIT: u64 = 16 * 1024 * 1024;
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(source)
        .context("Cannot open the selected project image")?;
    let metadata = input.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= FILE_LIMIT,
        "Choose an image file up to 16 MiB"
    );
    let mut bytes = Vec::new();
    (&mut input).take(FILE_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= FILE_LIMIT,
        "Project image exceeds 16 MiB"
    );
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().context(
        "Choose a valid PNG, JPEG, WebP, GIF, BMP, TIFF or ICO image up to 4096 × 4096 pixels",
    )?;
    let mut png = Cursor::new(Vec::new());
    image
        .thumbnail(256, 256)
        .write_to(&mut png, image::ImageFormat::Png)?;
    let directory = root.join("project-icons");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let path = directory.join(format!("{}.png", kodem_core::new_id()));
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)?;
    if let Err(error) = output
        .write_all(png.get_ref())
        .and_then(|_| output.sync_all())
    {
        let _ = fs::remove_file(&path);
        return Err(error.into());
    }
    Ok(path)
}
