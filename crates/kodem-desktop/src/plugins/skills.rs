//! Skills: folders that hold a `SKILL.md`, in the user-level skill folders that each CLI reads.
//!
//! Each provider has a folder of its own, where Kodem installs. Some CLIs also read the folders
//! of others: OpenCode and Grok Build read Claude Code's, and Codex, OpenCode and Grok Build read
//! the shared `~/.agents/skills`. A skill in such a folder reaches those providers too, which
//! the inventory reports as shared.
use anyhow::{Context, Result, ensure};
use kodem_core::{Provider, platform};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Most skills that one folder lists.
const FOLDER_LIMIT: usize = 1000;
/// Most bytes that one installed skill may hold, which bounds a copy from a large repository.
const SKILL_BYTES: u64 = 64 * 1024 * 1024;
/// Most files that one installed skill may hold.
const SKILL_FILES: usize = 4000;

/// A user-level folder of skills.
#[derive(Clone, Debug)]
pub struct Folder {
    pub path: PathBuf,
    /// The provider that owns the folder; absent for a folder that several CLIs share.
    pub owner: Option<Provider>,
    /// The providers that read the folder.
    pub readers: Vec<Provider>,
}

impl Folder {
    /// Returns the folder's name for a reader: its owner's name or that of the shared folder.
    pub fn label(&self) -> &'static str {
        match self.owner {
            Some(owner) => owner.label(),
            None => "Shared agents folder",
        }
    }
}

/// One copy of a skill in a folder.
#[derive(Clone, Debug)]
pub struct Copy {
    /// Index of the folder in `Inventory::folders`.
    pub folder: usize,
    /// The skill's own folder, which holds its `SKILL.md`.
    pub path: PathBuf,
}

/// A skill and its copies, by name.
#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub copies: Vec<Copy>,
}

/// How a provider reaches a skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// A copy is in the provider's own folder.
    Own,
    /// The provider reads a copy in another folder, which the label names.
    Shared(&'static str),
    /// The provider does not see the skill.
    Missing,
}

/// The skill folders and the skills in them.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub folders: Vec<Folder>,
    pub skills: Vec<Skill>,
}

impl Inventory {
    /// Returns how `provider` reaches `skill`.
    pub fn reach(&self, skill: &Skill, provider: Provider) -> Reach {
        let mut shared = None;
        for copy in &skill.copies {
            let folder = &self.folders[copy.folder];
            if folder.owner == Some(provider) {
                return Reach::Own;
            }
            if folder.readers.contains(&provider) {
                shared = Some(folder.label());
            }
        }
        shared.map_or(Reach::Missing, Reach::Shared)
    }

    /// Returns the copy of `skill` in the folder of `provider`.
    pub fn own_copy<'a>(&self, skill: &'a Skill, provider: Provider) -> Option<&'a Copy> {
        skill
            .copies
            .iter()
            .find(|copy| self.folders[copy.folder].owner == Some(provider))
    }
}

/// Returns the folder where Kodem installs the skills of `provider`.
pub fn own_folder(provider: Provider) -> Result<PathBuf> {
    let home = platform::home_dir()?;
    Ok(match provider {
        Provider::Claude => home.join(".claude").join("skills"),
        Provider::Codex => home.join(".codex").join("skills"),
        Provider::OpenCode => super::opencode_config_dir()?.join("skills"),
        Provider::Antigravity => home.join(".gemini").join("config").join("skills"),
        Provider::Grok => home.join(".grok").join("skills"),
    })
}

/// Returns the user-level skill folders: each provider's own, then the shared one.
pub fn folders() -> Vec<Folder> {
    let mut folders: Vec<Folder> = Provider::ALL
        .into_iter()
        .filter_map(|provider| {
            let readers = match provider {
                Provider::Claude => vec![Provider::Claude, Provider::OpenCode, Provider::Grok],
                _ => vec![provider],
            };
            Some(Folder {
                path: own_folder(provider).ok()?,
                owner: Some(provider),
                readers,
            })
        })
        .collect();
    if let Ok(home) = platform::home_dir() {
        folders.push(Folder {
            path: home.join(".agents").join("skills"),
            owner: None,
            readers: vec![Provider::Codex, Provider::OpenCode, Provider::Grok],
        });
    }
    folders
}

/// Lists the skills of every folder, with each skill's copies.
pub fn read() -> Inventory {
    let folders = folders();
    let mut skills: Vec<Skill> = Vec::new();
    for (index, folder) in folders.iter().enumerate() {
        let Ok(entries) = fs::read_dir(&folder.path) else {
            continue;
        };
        for entry in entries.flatten().take(FOLDER_LIMIT) {
            let path = entry.path();
            let manifest = path.join("SKILL.md");
            if !manifest.is_file() {
                continue;
            }
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            let (name, description) = manifest_fields(&manifest);
            let name = name.unwrap_or(folder_name);
            let copy = Copy {
                folder: index,
                path,
            };
            match skills.iter_mut().find(|skill| skill.name == name) {
                Some(skill) => {
                    if skill.description.is_empty() {
                        skill.description = description;
                    }
                    skill.copies.push(copy);
                }
                None => skills.push(Skill {
                    name,
                    description,
                    copies: vec![copy],
                }),
            }
        }
    }
    skills.sort_by_key(|skill| skill.name.to_lowercase());
    Inventory { folders, skills }
}

/// Reads the name and description of a skill's `SKILL.md` front matter.
pub fn manifest_fields(manifest: &Path) -> (Option<String>, String) {
    let mut text = String::new();
    if let Ok(file) = fs::File::open(manifest) {
        use std::io::Read;
        let _ = file.take(64 * 1024).read_to_string(&mut text);
    }
    front_matter(&text)
}

/// Reads `name` and `description` of YAML front matter, including folded and literal block
/// values, without a YAML parser: skills keep these two keys simple.
pub fn front_matter(text: &str) -> (Option<String>, String) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, String::new());
    }
    let mut name = None;
    let mut description = String::new();
    let mut block: Option<&str> = None;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(key) = block {
            if line.starts_with(' ') || line.starts_with('\t') || line.trim().is_empty() {
                if key == "description" {
                    if !description.is_empty() {
                        description.push(' ');
                    }
                    description.push_str(line.trim());
                }
                continue;
            }
            block = None;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = unquote(value.trim());
        match key.trim() {
            "name" => name = Some(value.to_owned()).filter(|name| !name.is_empty()),
            "description" => {
                if matches!(value, ">" | ">-" | "|" | "|-" | ">+" | "|+" | "") {
                    block = Some("description");
                } else {
                    description = value.to_owned();
                }
            }
            _ => {}
        }
    }
    (name, description.trim().to_owned())
}

// Removes one pair of matching quotes around a YAML scalar.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// Checks the name of a skill's folder: letters, digits, hyphens, underscores and dots, as the
/// CLIs accept, without a path.
pub fn validate_name(name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "Give the skill a name");
    ensure!(name.len() <= 64, "Use a name of at most 64 characters");
    ensure!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !name.starts_with('.'),
        "Use only letters, digits, hyphens and underscores in the name"
    );
    Ok(())
}

/// Copies the skill folder `source` into the folder of each provider in `providers`, under
/// `name`. A copy of the same name is replaced. Returns the failures, one per provider.
pub fn install(source: &Path, name: &str, providers: &[Provider]) -> Vec<(Provider, String)> {
    let mut failures = Vec::new();
    for provider in providers {
        let result = validate_name(name).and_then(|()| {
            ensure!(
                source.join("SKILL.md").is_file(),
                "The folder holds no SKILL.md"
            );
            let target = own_folder(*provider)?.join(name);
            replace_with_copy(source, &target)
        });
        if let Err(error) = result {
            failures.push((*provider, format!("{error:#}")));
        }
    }
    failures
}

/// Writes a new skill with `description` and the instructions `body` into the folder of each
/// provider in `providers`. Returns the failures, one per provider.
pub fn create(
    name: &str,
    description: &str,
    body: &str,
    providers: &[Provider],
) -> Vec<(Provider, String)> {
    let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let manifest = format!(
        "---\nname: {name}\ndescription: {}\n---\n\n{}\n",
        yaml_scalar(&description),
        body.trim()
    );
    let mut failures = Vec::new();
    for provider in providers {
        let result = validate_name(name).and_then(|()| {
            let folder = own_folder(*provider)?.join(name);
            ensure!(
                !folder.exists(),
                "A skill named {name} is already in this folder"
            );
            fs::create_dir_all(&folder)?;
            fs::write(folder.join("SKILL.md"), &manifest)?;
            Ok(())
        });
        if let Err(error) = result {
            failures.push((*provider, format!("{error:#}")));
        }
    }
    failures
}

// Quotes a YAML scalar when it could be read as something else.
fn yaml_scalar(text: &str) -> String {
    let plain = !text.is_empty()
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.starts_with(|c: char| "!&*[]{}|>'\"%@`#,?:-".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Removes a skill's copy from the folder of each provider in `providers`. A copy in a shared
/// folder stays. Returns the failures, one per provider.
pub fn remove(
    inventory: &Inventory,
    skill: &Skill,
    providers: &[Provider],
) -> Vec<(Provider, String)> {
    let mut failures = Vec::new();
    for provider in providers {
        if let Some(copy) = inventory.own_copy(skill, *provider)
            && let Err(error) = remove_folder(&copy.path)
        {
            failures.push((*provider, format!("{error:#}")));
        }
    }
    failures
}

/// Removes every copy of a skill, in every folder.
pub fn remove_everywhere(skill: &Skill) -> Result<()> {
    for copy in &skill.copies {
        remove_folder(&copy.path)?;
    }
    Ok(())
}

// Removes a skill's folder, or the link that stands for it, after checking that it is one.
fn remove_folder(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        // A linked skill: the link goes, what it links to stays.
        #[cfg(windows)]
        {
            if fs::remove_dir(path).is_ok() {
                return Ok(());
            }
        }
        return fs::remove_file(path).with_context(|| format!("Cannot remove {}", path.display()));
    }
    ensure!(
        path.join("SKILL.md").is_file(),
        "{} is not a skill folder",
        path.display()
    );
    fs::remove_dir_all(path).with_context(|| format!("Cannot remove {}", path.display()))
}

// Replaces `target` with a copy of `source`. The copy is made beside the target first, so a
// failed copy leaves the earlier skill in place.
fn replace_with_copy(source: &Path, target: &Path) -> Result<()> {
    let source = super::resolved(source);
    if super::resolved(target) == source {
        return Ok(());
    }
    ensure!(
        !target.starts_with(&source),
        "A skill cannot be copied into itself"
    );
    let parent = target.parent().context("The skill folder has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Cannot create the folder {}", parent.display()))?;
    let staged = tempfile::Builder::new()
        .prefix(".kodem-skill-")
        .tempdir_in(parent)?;
    let mut budget = (SKILL_FILES, SKILL_BYTES);
    copy_tree(&source, staged.path(), &mut budget)?;
    if fs::symlink_metadata(target).is_ok() {
        remove_folder(target)?;
    }
    let staged = staged.keep();
    fs::rename(&staged, target).inspect_err(|_| {
        let _ = fs::remove_dir_all(&staged);
    })?;
    Ok(())
}

// Copies a folder's files and subfolders, without version control folders and without links,
// within a budget of files and bytes.
fn copy_tree(source: &Path, target: &Path, budget: &mut (usize, u64)) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        let kind = entry.file_type()?;
        let from = entry.path();
        let to = target.join(&name);
        if kind.is_dir() {
            copy_tree(&from, &to, budget)?;
        } else if kind.is_file() {
            let size = entry.metadata()?.len();
            ensure!(
                budget.0 > 0 && budget.1 >= size,
                "The skill is too large to install"
            );
            budget.0 -= 1;
            budget.1 -= size;
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Reads a skill's `SKILL.md`, bounded for display.
pub fn read_manifest(skill_folder: &Path) -> Result<String> {
    use std::io::Read;
    let mut text = String::new();
    fs::File::open(skill_folder.join("SKILL.md"))?
        .take(256 * 1024)
        .read_to_string(&mut text)?;
    Ok(text)
}

/// Lists the files of a skill's folder relative to it, at most `limit`, for display.
pub fn files(skill_folder: &Path, limit: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![skill_folder.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= limit {
                return found;
            }
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(skill_folder) {
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}
