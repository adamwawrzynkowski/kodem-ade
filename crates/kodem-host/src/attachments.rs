//! Imports immutable user-selected files, and references folders, without blocking the host
//! state lock.
use anyhow::{Context, Result, ensure};
use kodem_core::*;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

const FILE_LIMIT: u64 = 32 * 1024 * 1024;
const BATCH_LIMIT: u64 = 128 * 1024 * 1024;

/// Copies a bounded batch of files into private storage and removes partial imports on
/// failure. A folder is not copied: its attachment refers to the folder in place.
pub(super) fn import(root: &Path, sources: Vec<AttachmentSource>) -> Result<Vec<Attachment>> {
    ensure!(
        !sources.is_empty() && sources.len() <= 32,
        "Attach between 1 and 32 files or folders"
    );
    let mut imported = Vec::new();
    let mut created = Vec::new();
    let result = (|| {
        let mut total = 0;
        for selected in sources {
            let source = selected.path();
            let skill = if matches!(selected, AttachmentSource::Skill(_)) {
                ensure!(
                    source.file_name().is_some_and(|name| name == "SKILL.md"),
                    "Select a SKILL.md file"
                );
                Some(
                    source
                        .canonicalize()
                        .map(kodem_core::platform::plain_path)
                        .context("Skill source is unavailable")?,
                )
            } else {
                None
            };
            let mut input = platform::open_nonblocking(source)
                .with_context(|| format!("Cannot open {}", source.display()))?;
            let meta = input.metadata()?;
            if meta.is_dir() && skill.is_none() {
                // A folder can exceed every attachment limit, so the agent reads it in place.
                let path = source
                    .canonicalize()
                    .map(kodem_core::platform::plain_path)
                    .with_context(|| format!("Cannot open {}", source.display()))?;
                let name = path
                    .file_name()
                    .context("Attachment has no filename")?
                    .to_string_lossy()
                    .into_owned();
                imported.push(Attachment {
                    id: new_id(),
                    name,
                    path,
                    bytes: 0,
                    kind: AttachmentKind::Folder,
                });
                continue;
            }
            ensure!(
                meta.is_file(),
                "Attach a file or a folder: {}",
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
            let kind = if let Some(source) = skill {
                AttachmentKind::Skill { source }
            } else if AttachmentKind::is_image_header(&bytes) {
                AttachmentKind::Image
            } else {
                AttachmentKind::File
            };
            let id = new_id();
            let directory = root.join("attachments").join(&id);
            fs::create_dir_all(&directory)?;
            created.push(directory.clone());
            platform::restrict(&directory, 0o700)?;
            let name = source
                .file_name()
                .context("Attachment has no filename")?
                .to_string_lossy()
                .into_owned();
            let path = directory.join(&name);
            let mut output = platform::create_private(&path)?;
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

/// Removes only freshly imported private copies when registering a batch fails.
pub(super) fn discard(root: &Path, imported: &[Attachment]) {
    for attachment in imported {
        // A referenced folder belongs to the user. Remove the private copy by its identity,
        // never by the attachment path.
        let _ = fs::remove_dir_all(root.join("attachments").join(&attachment.id));
    }
}
