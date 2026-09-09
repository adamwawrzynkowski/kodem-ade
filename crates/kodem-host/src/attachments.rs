//! Imports immutable user-selected files without blocking the host state lock.
use anyhow::{Context, Result, ensure};
use kodem_core::*;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const FILE_LIMIT: u64 = 32 * 1024 * 1024;
const BATCH_LIMIT: u64 = 128 * 1024 * 1024;

/// Copies a bounded batch into private storage and removes partial imports on failure.
pub(super) fn import(root: &Path, paths: Vec<PathBuf>) -> Result<Vec<Attachment>> {
    ensure!(
        !paths.is_empty() && paths.len() <= 32,
        "Drop between 1 and 32 files"
    );
    let mut imported = Vec::new();
    let mut created = Vec::new();
    let result = (|| {
        let mut total = 0;
        for source in paths {
            let mut input = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&source)
                .with_context(|| format!("Cannot open {}", source.display()))?;
            let meta = input.metadata()?;
            ensure!(
                meta.is_file(),
                "Drop files rather than folders: {}",
                source.display()
            );
            ensure!(
                meta.len() <= FILE_LIMIT,
                "{} exceeds the 32 MiB attachment limit",
                source.display()
            );
            let mut bytes = Vec::new();
            (&mut input).take(FILE_LIMIT + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 <= FILE_LIMIT,
                "Attachment grew beyond 32 MiB while importing"
            );
            total += bytes.len() as u64;
            ensure!(total <= BATCH_LIMIT, "Attachment batch exceeds 128 MiB");
            let kind = if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                || bytes.starts_with(&[0xff, 0xd8, 0xff])
                || bytes.starts_with(b"GIF87a")
                || bytes.starts_with(b"GIF89a")
                || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
            {
                AttachmentKind::Image
            } else {
                AttachmentKind::File
            };
            let id = new_id();
            let directory = root.join("attachments").join(&id);
            fs::create_dir_all(&directory)?;
            created.push(directory.clone());
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            let name = source
                .file_name()
                .context("Attachment has no filename")?
                .to_string_lossy()
                .into_owned();
            let path = directory.join(&name);
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)?;
            output.write_all(&bytes)?;
            output.sync_all()?;
            imported.push(Attachment {
                id,
                name,
                path,
                bytes: bytes.len() as u64,
                kind,
            });
        }
        Ok(imported)
    })();
    if result.is_err() {
        for directory in created {
            let _ = fs::remove_dir_all(directory);
        }
    }
    result
}

/// Removes only freshly imported directories when registering a batch fails.
pub(super) fn discard(imported: &[Attachment]) {
    for attachment in imported {
        if let Some(directory) = attachment.path.parent() {
            let _ = fs::remove_dir_all(directory);
        }
    }
}
