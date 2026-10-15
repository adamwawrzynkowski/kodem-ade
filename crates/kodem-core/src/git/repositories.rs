//! Discovers related working trees from Git paths and local Unity package references.
use super::*;
use std::collections::BTreeSet;

/// Finds nested repositories, submodules, symlinked checkouts and local Unity dependencies.
/// Canonical roots deduplicate package links and stop cycles between repositories.
pub fn repositories(cwd: &Path) -> Result<Vec<PathBuf>> {
    let root = repository(cwd)?
        .canonicalize()
        .map(crate::platform::plain_path)?;
    let mut roots = vec![root.clone()];
    let mut seen = BTreeSet::from([root]);
    let mut cursor = 0;
    while cursor < roots.len() {
        let cwd = roots[cursor].clone();
        cursor += 1;
        let mut candidates = BTreeSet::new();
        // Only gitlinks and symlinks can link tracked content to another checkout.
        // Read their modes from the index instead of statting every tracked source file.
        for entry in query(&cwd, &["ls-files", "--stage", "-z"])?
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
        {
            if (entry.starts_with(b"160000 ") || entry.starts_with(b"120000 "))
                && let Some(tab) = entry.iter().position(|b| *b == b'\t')
            {
                let path = cwd.join(platform::os_string(&entry[tab + 1..]));
                if path.is_dir() {
                    candidates.insert(path);
                }
            }
        }
        // Git stops at untracked nested checkouts and excludes generated output.
        let paths = query(&cwd, &["ls-files", "-z", "--others", "--exclude-standard"])?;
        let mut directories = BTreeSet::new();
        for path in paths.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let full = cwd.join(platform::os_string(path));
            for ancestor in full.ancestors().take_while(|p| *p != cwd) {
                if !directories.insert(ancestor.to_path_buf()) {
                    break;
                }
                if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                    if ancestor.is_dir() {
                        candidates.insert(ancestor.to_path_buf());
                    }
                } else if ancestor.join(".git").exists() {
                    candidates.insert(ancestor.to_path_buf());
                }
            }
        }
        // An ignored checkout can still be an intentional nested dependency. Inspect only
        // each ignored directory's root, never recurse into caches such as Unity's Library.
        for path in query(
            &cwd,
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ],
        )?
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        {
            let full = cwd.join(platform::os_string(path));
            if full.join(".git").exists() {
                candidates.insert(full);
            }
        }
        let manifest = cwd.join("Packages/manifest.json");
        if manifest.is_file() {
            let primary = primary_checkout(&cwd);
            let value: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)
                .with_context(|| {
                    format!("Read local package references in {}", manifest.display())
                })?;
            if let Some(dependencies) = value["dependencies"].as_object() {
                for dependency in dependencies.values().filter_map(|v| v.as_str()) {
                    if let Some(path) = dependency.strip_prefix("file:") {
                        let local = cwd.join("Packages").join(path);
                        if local.is_dir() {
                            candidates.insert(local);
                        } else if let Some(checkout) = &primary {
                            // Task worktrees relocate the manifest but do not relocate sibling
                            // package repositories. Resolve absent links from the original checkout.
                            let local = checkout.join("Packages").join(path);
                            if local.is_dir() {
                                candidates.insert(local);
                            }
                        }
                    }
                }
            }
        }
        // Hundreds of local packages may belong to one repository. Resolve their nearest
        // .git ancestor before querying Git so discovery costs scale with checkouts.
        let candidates: BTreeSet<_> = candidates
            .into_iter()
            .filter_map(|path| {
                let canonical = path.canonicalize().map(crate::platform::plain_path).ok()?;
                canonical
                    .ancestors()
                    .find(|p| p.join(".git").exists())
                    .map(Path::to_path_buf)
            })
            .collect();
        for candidate in candidates {
            if seen.contains(&candidate) {
                continue;
            }
            let resolved = repository(&candidate)?
                .canonicalize()
                .map(crate::platform::plain_path)?;
            if seen.insert(resolved.clone()) {
                roots.push(resolved);
            }
        }
    }
    roots[1..].sort();
    Ok(roots)
}

fn primary_checkout(cwd: &Path) -> Option<PathBuf> {
    let bytes = query(cwd, &["rev-parse", "--git-common-dir"]).ok()?;
    let common = cwd
        .join(platform::os_string(
            bytes.strip_suffix(b"\n").unwrap_or(&bytes),
        ))
        .canonicalize()
        .map(crate::platform::plain_path)
        .ok()?;
    (common.file_name()? == ".git").then(|| common.parent().unwrap().to_path_buf())
}

/// Records each linked checkout's own starting commit without changing its working tree.
/// A folder without Git links no checkouts.
pub fn repository_baselines(cwd: &Path) -> Result<Vec<RepositoryBaseline>> {
    if !is_repository(cwd) {
        return Ok(Vec::new());
    }
    Ok(repositories(cwd)?
        .into_iter()
        .skip(1)
        .map(|root| RepositoryBaseline {
            head: head(&root),
            root,
        })
        .collect())
}

/// Names an owning checkout, retaining the relative location of nested repositories.
pub(super) fn label(root: &Path, main: &Path) -> String {
    if root != main
        && let Ok(relative) = root.strip_prefix(main)
    {
        return relative.to_string_lossy().into_owned();
    }
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}
