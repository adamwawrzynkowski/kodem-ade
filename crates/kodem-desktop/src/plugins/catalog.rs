//! Catalogs that offer MCP servers and skills to install: the official MCP Registry, skill
//! repositories on GitHub, and a search of GitHub for servers that the registry does not list.
//!
//! The registry is read in pages and kept on disk, so the next start shows it at once and then
//! reads it again. A skill repository is read as one archive, which also gives every skill's
//! files for its installation.
use super::{mcp, skills};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

/// The address of the official MCP Registry's list of servers.
const REGISTRY: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";
/// Servers per page of the registry, its largest page.
const PAGE_SIZE: usize = 100;
/// Most pages read in one refresh, which bounds the time and the size of the list.
pub const PAGE_LIMIT: usize = 60;
/// Most bytes of one skill repository's archive.
const ARCHIVE_BYTES: u64 = 192 * 1024 * 1024;

/// A variable or header that a server asks the user for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub secret: bool,
    /// The value that the catalog proposes, such as `Bearer {token}`.
    pub default: String,
}

/// One way to run or reach a catalog server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Launch {
    /// A package that a runner starts locally, such as `npx -y @scope/server`.
    Package {
        /// The package registry: npm, pypi, oci, nuget or mcpb.
        registry: String,
        identifier: String,
        version: String,
        /// The words that start the package, with the version where the runner takes one.
        command: String,
        args: Vec<String>,
        env: Vec<Input>,
    },
    /// A hosted server.
    Remote {
        kind: RemoteKind,
        url: String,
        headers: Vec<Input>,
    },
}

/// The transport of a hosted server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteKind {
    Http,
    Sse,
}

impl Launch {
    /// Returns a short label of how the server runs.
    pub fn label(&self) -> String {
        match self {
            Self::Package { registry, .. } => match registry.as_str() {
                "npm" => "npm package".into(),
                "pypi" => "Python package".into(),
                "oci" => "Docker image".into(),
                "nuget" => ".NET package".into(),
                other => format!("{other} package"),
            },
            Self::Remote {
                kind: RemoteKind::Sse,
                ..
            } => "Hosted · SSE".into(),
            Self::Remote { .. } => "Hosted · HTTP".into(),
        }
    }

    /// Returns the values that the server asks for.
    pub fn inputs(&self) -> &[Input] {
        match self {
            Self::Package { env, .. } => env,
            Self::Remote { headers, .. } => headers,
        }
    }

    /// Returns the transport of the server with the values that the user gave, by input name.
    /// An input without a value keeps the catalog's proposal, or is left out when it has none.
    pub fn transport(&self, values: &[(String, String)]) -> mcp::Transport {
        let resolve = |inputs: &[Input]| -> Vec<(String, String)> {
            inputs
                .iter()
                .filter_map(|input| {
                    let given = values
                        .iter()
                        .find(|(name, _)| name == &input.name)
                        .map(|(_, value)| value.trim().to_owned())
                        .filter(|value| !value.is_empty());
                    let value = match given {
                        // A header such as `Bearer {token}` takes the value in its placeholder.
                        Some(value) if input.default.contains('{') && !value.contains(' ') => {
                            fill_placeholder(&input.default, &value)
                        }
                        Some(value) => value,
                        None if !input.default.is_empty() && !input.default.contains('{') => {
                            input.default.clone()
                        }
                        None => return None,
                    };
                    Some((input.name.clone(), value))
                })
                .collect()
        };
        match self {
            Self::Package {
                command, args, env, ..
            } => mcp::Transport::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: resolve(env),
            },
            Self::Remote { kind, url, headers } => {
                let headers = resolve(headers);
                match kind {
                    RemoteKind::Http => mcp::Transport::Http {
                        url: url.clone(),
                        headers,
                    },
                    RemoteKind::Sse => mcp::Transport::Sse {
                        url: url.clone(),
                        headers,
                    },
                }
            }
        }
    }
}

// Replaces the first `{…}` placeholder of a template with a value.
fn fill_placeholder(template: &str, value: &str) -> String {
    match (template.find('{'), template.find('}')) {
        (Some(start), Some(end)) if end > start => {
            format!("{}{value}{}", &template[..start], &template[end + 1..])
        }
        _ => value.to_owned(),
    }
}

/// A server of the MCP Registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogServer {
    /// The registry's name, such as `io.github.owner/server`.
    pub name: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub repository: String,
    pub website: String,
    /// The ways to run it, hosted servers first.
    pub launches: Vec<Launch>,
    /// When the registry last saw a new version, in RFC 3339.
    pub updated: String,
    /// The address of the server's own icon, when the registry names one.
    #[serde(default)]
    pub icon: String,
}

impl CatalogServer {
    /// Returns the image that stands for the server: its own icon, else the avatar of the GitHub
    /// account that publishes it.
    pub fn image(&self) -> Option<String> {
        if !self.icon.is_empty() {
            return Some(self.icon.clone());
        }
        github_owner(&self.repository)
            .or_else(|| {
                let namespace = self.name.split('/').next()?;
                namespace.strip_prefix("io.github.").map(str::to_owned)
            })
            .map(|owner| avatar(&owner))
    }

    /// Returns the name to show: the title, else the last part of the registry name.
    pub fn display_name(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.trim().to_owned();
        }
        let last = self.name.rsplit('/').next().unwrap_or(&self.name);
        last.split(['-', '_'])
            .filter(|word| !word.is_empty())
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Returns who publishes the server: the namespace of its registry name, such as
    /// `github.com/owner` for `io.github.owner/server`.
    pub fn publisher(&self) -> String {
        let namespace = self.name.split('/').next().unwrap_or(&self.name);
        let mut parts: Vec<&str> = namespace.split('.').collect();
        if parts.len() >= 3 && parts[0] == "io" && parts[1] == "github" {
            return format!("github.com/{}", parts[2..].join("."));
        }
        parts.reverse();
        parts.join(".")
    }

    /// Reports whether the query matches the server: every word, in lower case, must be in its
    /// name, title or description.
    pub fn matches(&self, words: &[String]) -> bool {
        if words.is_empty() {
            return true;
        }
        let haystack =
            format!("{}\n{}\n{}", self.name, self.title, self.description).to_lowercase();
        words.iter().all(|word| haystack.contains(word.as_str()))
    }
}

/// Returns the account of a GitHub address, such as `owner` of `https://github.com/owner/repo`.
pub fn github_owner(address: &str) -> Option<String> {
    let rest = address
        .strip_prefix("https://github.com/")
        .or_else(|| address.strip_prefix("http://github.com/"))?;
    let owner = rest.split('/').next()?;
    valid_segment(owner).then(|| owner.to_owned())
}

/// Returns the address of a GitHub account's avatar.
pub fn avatar(owner: &str) -> String {
    format!("https://github.com/{owner}.png?size=96")
}

/// Most bytes of one icon.
const ICON_BYTES: u64 = 1024 * 1024;

/// Returns an icon on this computer: the copy that an earlier read kept, else a new download.
/// The file's extension follows its content, which is how GPUI chooses its decoder.
pub fn fetch_image(url: &str) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let folder = kodem_core::ipc::data_dir()?.join("plugins").join("icons");
    let digest = Sha256::digest(url.as_bytes());
    let stem: String = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    for extension in IMAGE_EXTENSIONS {
        let kept = folder.join(format!("{stem}.{extension}"));
        if kept.is_file() {
            return Ok(kept);
        }
    }
    fs::create_dir_all(&folder)?;
    let output = crate::harness::curl(url, "image/*", Duration::from_secs(20), ICON_BYTES)
        .stderr(Stdio::null())
        .output()
        .context("curl did not start")?;
    ensure!(output.status.success(), "The icon could not be read");
    let extension = image_extension(&output.stdout).context("The icon is not an image")?;
    let path = folder.join(format!("{stem}.{extension}"));
    super::write_atomically(&path, &output.stdout)?;
    Ok(path)
}

/// The image formats that an icon can have, as file extensions.
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "gif", "webp", "svg", "ico"];

// Returns the extension of an image's format from its first bytes.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        return Some("png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(&[0, 0, 1, 0]) {
        return Some("ico");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
    (head.contains("<svg")).then_some("svg")
}

/// The registry's servers as Kodem last read them.
///
/// The first read takes every page, which can take minutes, so the list is kept after each
/// page with the cursor of the next one; a later start continues there. Once complete, the list
/// is brought up to date with the servers that changed since it was read.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    pub servers: Vec<CatalogServer>,
    /// When the list was last complete and up to date, in seconds since the Unix epoch; zero
    /// until the first read is complete.
    pub read_at: u64,
    /// The cursor of the next page of an unfinished first read.
    #[serde(default)]
    pub cursor: Option<String>,
    /// When the unfinished first read began, in seconds since the Unix epoch. The list is up
    /// to date as of then once the read completes.
    #[serde(default)]
    pub started_at: u64,
}

/// How long a complete list counts as up to date before it asks the registry for changes.
pub const FRESH_FOR: Duration = Duration::from_secs(12 * 60 * 60);

/// Returns the time now, in seconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// A server that changed in the registry: its new entry, or nothing when it was deleted,
/// deprecated or can no longer be run.
pub type Change = (String, Option<CatalogServer>);

/// Applies changes to a list: a new or updated server takes the place of its earlier entry,
/// and a server without an entry leaves the list. The list stays in the registry's order, by
/// name.
pub fn merge_changes(servers: &mut Vec<CatalogServer>, changes: Vec<Change>) {
    let mut changed: std::collections::HashMap<String, Option<CatalogServer>> =
        changes.into_iter().collect();
    servers.retain_mut(|server| match changed.remove(&server.name) {
        Some(Some(entry)) => {
            *server = entry;
            true
        }
        Some(None) => false,
        None => true,
    });
    servers.extend(changed.into_values().flatten());
    servers.sort_by(|a, b| a.name.cmp(&b.name));
}

// Returns the file that keeps the registry between starts.
fn registry_cache() -> Result<PathBuf> {
    Ok(kodem_core::ipc::data_dir()?
        .join("plugins")
        .join("mcp-registry.json"))
}

/// Returns the registry as the last refresh kept it, when there is one.
pub fn cached_registry() -> Option<Registry> {
    let bytes = fs::read(registry_cache().ok()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Reads one page of the registry's latest servers after `cursor`: every server, or with
/// `since` (seconds since the Unix epoch) only those that changed since then. Returns each
/// server by name with its entry, absent for one that cannot be offered, and the cursor of the
/// next page, absent after the last.
pub fn registry_page(
    cursor: Option<&str>,
    since: Option<u64>,
) -> Result<(Vec<Change>, Option<String>)> {
    let mut url = format!("{REGISTRY}?limit={PAGE_SIZE}&version=latest");
    if let Some(since) = since {
        let time = chrono::DateTime::from_timestamp(since as i64, 0)
            .context("The time of the last read is out of range")?;
        url.push_str("&updated_since=");
        url.push_str(&percent_encode(
            &time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ));
    }
    if let Some(cursor) = cursor {
        url.push_str("&cursor=");
        url.push_str(&percent_encode(cursor));
    }
    // The registry fails a page now and then; a second attempt usually gets it.
    let body = registry_fetch(&url)
        .or_else(|_| {
            std::thread::sleep(Duration::from_secs(2));
            registry_fetch(&url)
        })
        .context("The MCP Registry did not answer")?;
    let document: Value =
        serde_json::from_slice(&body).context("The MCP Registry sent an unreadable page")?;
    let changes = document["servers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let name = entry.get("server").unwrap_or(entry)["name"].as_str()?;
            Some((name.trim().to_owned(), catalog_server(entry)))
        })
        .filter(|(name, _)| !name.is_empty())
        .collect();
    let next = document["metadata"]["nextCursor"]
        .as_str()
        .map(str::to_owned)
        .filter(|cursor| !cursor.is_empty());
    Ok((changes, next))
}

// Reads a page of the registry. Its pages and its search can take half a minute.
fn registry_fetch(url: &str) -> Result<Vec<u8>> {
    let output = crate::harness::curl(
        url,
        "application/json",
        Duration::from_secs(90),
        16 * 1024 * 1024,
    )
    .stderr(Stdio::piped())
    .output()
    .context("curl did not start")?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

/// Keeps the registry's list on disk for the next start.
pub fn save_registry(registry: &Registry) -> Result<()> {
    let path = registry_cache()?;
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    super::write_atomically(&path, &serde_json::to_vec(registry)?)
}

/// Searches the registry by name, for a server that the kept list does not hold yet.
pub fn search_registry(query: &str) -> Result<Vec<CatalogServer>> {
    let url = format!(
        "{REGISTRY}?limit={PAGE_SIZE}&version=latest&search={}",
        percent_encode(query.trim())
    );
    let body = registry_fetch(&url).context("The MCP Registry did not answer")?;
    let document: Value = serde_json::from_slice(&body)?;
    Ok(document["servers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(catalog_server)
        .collect())
}

// Reads one entry of a registry page: `{ server, _meta }`. Deleted and deprecated servers and
// servers without a way to run them are left out.
fn catalog_server(entry: &Value) -> Option<CatalogServer> {
    let server = entry.get("server").unwrap_or(entry);
    let official = &entry["_meta"]["io.modelcontextprotocol.registry/official"];
    if let Some(status) = official["status"].as_str().or(server["status"].as_str())
        && status != "active"
    {
        return None;
    }
    if official["isLatest"].as_bool() == Some(false) {
        return None;
    }
    let text = |value: &Value| value.as_str().unwrap_or_default().trim().to_owned();
    let mut launches: Vec<Launch> = server["remotes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(remote_launch)
        .collect();
    launches.extend(
        server["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(package_launch),
    );
    if launches.is_empty() {
        return None;
    }
    Some(CatalogServer {
        name: text(&server["name"]),
        title: text(&server["title"]),
        description: text(&server["description"]),
        version: text(&server["version"]),
        repository: text(&server["repository"]["url"]),
        website: text(&server["websiteUrl"]),
        launches,
        updated: text(&official["updatedAt"]),
        icon: server["icons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|icon| icon["src"].as_str())
            .find(|src| src.starts_with("https://"))
            .unwrap_or_default()
            .to_owned(),
    })
}

// Reads a hosted server of a registry entry. A URL with a `{variable}` cannot be used as it is.
fn remote_launch(remote: &Value) -> Option<Launch> {
    let url = remote["url"].as_str()?.trim().to_owned();
    if url.contains('{') || !url.starts_with("http") {
        return None;
    }
    let kind = match remote["type"].as_str()? {
        "streamable-http" | "http" => RemoteKind::Http,
        "sse" => RemoteKind::Sse,
        _ => return None,
    };
    Some(Launch::Remote {
        kind,
        url,
        headers: inputs(&remote["headers"]),
    })
}

// Reads a package of a registry entry and the command that starts it. Only packages that talk
// over standard input and output, with a runner that Kodem knows, become a launch.
fn package_launch(package: &Value) -> Option<Launch> {
    let registry = package["registryType"].as_str()?.to_owned();
    let identifier = package["identifier"].as_str()?.trim().to_owned();
    let version = package["version"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let transport = package["transport"]["type"].as_str().unwrap_or("stdio");
    if transport != "stdio" || identifier.is_empty() {
        return None;
    }
    let pinned = |separator: &str| {
        if version.is_empty() || version == "latest" {
            identifier.clone()
        } else {
            format!("{identifier}{separator}{version}")
        }
    };
    let mut env = inputs(&package["environmentVariables"]);
    let runtime_args = arguments(&package["runtimeArguments"], &mut env);
    let package_args = arguments(&package["packageArguments"], &mut env);
    let (command, mut args) = match registry.as_str() {
        "npm" => {
            let mut args = vec!["-y".to_owned()];
            args.extend(runtime_args.into_iter().filter(|arg| arg != "-y"));
            args.push(pinned("@"));
            ("npx".to_owned(), args)
        }
        "pypi" => {
            let mut args = runtime_args;
            args.push(pinned("=="));
            ("uvx".to_owned(), args)
        }
        "oci" => {
            let mut args = vec!["run".to_owned(), "-i".to_owned(), "--rm".to_owned()];
            args.extend(runtime_args);
            // Docker passes each variable of the server's environment by name.
            for variable in env.iter() {
                args.push("-e".into());
                args.push(variable.name.clone());
            }
            let image = if identifier.contains(':') || version.is_empty() {
                identifier.clone()
            } else {
                format!("{identifier}:{version}")
            };
            args.push(image);
            ("docker".to_owned(), args)
        }
        "nuget" => {
            let mut args = vec!["dnx".to_owned()];
            args.extend(runtime_args);
            args.push(pinned("@"));
            args.push("--yes".into());
            ("dotnet".to_owned(), args)
        }
        _ => return None,
    };
    // A runtime hint such as `bunx` or `pnpm dlx` names another runner of the same package.
    let command = match package["runtimeHint"].as_str() {
        Some(hint @ ("bunx" | "pnpx" | "uvx" | "docker" | "npx")) => hint.to_owned(),
        _ => command,
    };
    args.extend(package_args);
    Some(Launch::Package {
        registry,
        identifier,
        version,
        command,
        args,
        env,
    })
}

// Reads the variables or headers that a registry entry asks for.
fn inputs(list: &Value) -> Vec<Input> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|input| {
            Some(Input {
                name: input["name"].as_str()?.trim().to_owned(),
                description: input["description"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
                required: input["isRequired"].as_bool().unwrap_or(false),
                secret: input["isSecret"].as_bool().unwrap_or(false),
                default: input["value"]
                    .as_str()
                    .or_else(|| input["default"].as_str())
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .filter(|input| !input.name.is_empty())
        .collect()
}

// Reads the arguments of a registry package that have a fixed value: positional values, and
// named options with their value. An option `-e NAME={variable}` asks for a value: `NAME` joins
// the variables in `env` and the option passes it by name. Other arguments that ask the user
// are left out.
fn arguments(list: &Value, env: &mut Vec<Input>) -> Vec<String> {
    let mut args = Vec::new();
    for argument in list.as_array().into_iter().flatten() {
        let raw = argument["value"]
            .as_str()
            .or_else(|| argument["default"].as_str());
        let value = raw.filter(|value| !value.contains('{'));
        match argument["type"].as_str() {
            Some("named") => {
                let Some(name) = argument["name"].as_str() else {
                    continue;
                };
                match (value, raw) {
                    (Some(value), _) => {
                        args.push(name.to_owned());
                        args.push(value.to_owned());
                    }
                    (None, Some(template)) if matches!(name, "-e" | "--env") => {
                        let Some((key, _)) = template.split_once('=') else {
                            continue;
                        };
                        if !env.iter().any(|input| input.name == key) {
                            let secret =
                                argument["variables"].as_object().is_some_and(|variables| {
                                    variables.values().any(|variable| {
                                        variable["isSecret"].as_bool() == Some(true)
                                    })
                                });
                            env.push(Input {
                                name: key.to_owned(),
                                description: argument["description"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_owned(),
                                required: argument["isRequired"].as_bool().unwrap_or(true),
                                secret,
                                default: String::new(),
                            });
                        }
                    }
                    (None, _) if argument["isRequired"].as_bool() != Some(true) => {}
                    (None, Some(_)) => {}
                    (None, None) => args.push(name.to_owned()),
                }
            }
            _ => {
                if let Some(value) = value {
                    args.push(value.to_owned());
                }
            }
        }
    }
    args
}

// Encodes text for a query parameter.
fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// A GitHub repository that offers skills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillSource {
    pub label: &'static str,
    pub owner: &'static str,
    pub repository: &'static str,
    /// The folder of the repository that holds the skills.
    pub folder: &'static str,
}

/// The skill repositories that the Skills page offers.
pub const SKILL_SOURCES: [SkillSource; 2] = [
    SkillSource {
        label: "Anthropic",
        owner: "anthropics",
        repository: "skills",
        folder: "skills",
    },
    SkillSource {
        label: "OpenAI",
        owner: "openai",
        repository: "skills",
        folder: "skills/.curated",
    },
];

/// A skill that a repository offers, with its files unpacked on this computer.
#[derive(Clone, Debug)]
pub struct CatalogSkill {
    pub name: String,
    pub description: String,
    /// The skill's folder in the repository, such as `skills/pdf`.
    pub path: String,
    /// The address of the skill's folder on GitHub.
    pub url: String,
    /// The unpacked folder, which an installation copies.
    pub folder: PathBuf,
    /// Keeps the unpacked repository until the last skill of it is dropped; absent for a
    /// folder on this computer.
    pub _archive: Option<Arc<tempfile::TempDir>>,
}

/// Downloads a repository of skills and lists the skills under `folder`, or under the whole
/// repository when the folder is empty. `reference` is a branch, a tag or a commit; empty is
/// the default branch.
pub fn read_skill_repository(
    owner: &str,
    repository: &str,
    reference: &str,
    folder: &str,
) -> Result<Vec<CatalogSkill>> {
    ensure!(
        valid_segment(owner) && valid_segment(repository),
        "This is not the address of a GitHub repository"
    );
    let reference = if reference.is_empty() {
        "HEAD"
    } else {
        reference
    };
    let archive = Arc::new(tempfile::Builder::new().prefix("kodem-skills-").tempdir()?);
    let file = archive.path().join("repository.tar.gz");
    let url = format!("https://codeload.github.com/{owner}/{repository}/tar.gz/{reference}");
    let output = crate::harness::curl(
        &url,
        "application/octet-stream",
        Duration::from_secs(180),
        ARCHIVE_BYTES,
    )
    .arg("--output")
    .arg(&file)
    .stderr(Stdio::piped())
    .output()
    .context("curl did not start")?;
    ensure!(
        output.status.success(),
        "GitHub did not send {owner}/{repository}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let unpacked = archive.path().join("unpacked");
    fs::create_dir_all(&unpacked)?;
    let mut tar = std::process::Command::new(crate::harness::tar_program()?);
    kodem_core::platform::hide_console(&mut tar);
    let status = tar
        .arg("-xzf")
        .arg(&file)
        .arg("-C")
        .arg(&unpacked)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("tar did not start")?;
    ensure!(status.success(), "The archive of the repository is damaged");
    let _ = fs::remove_file(&file);
    // The archive holds one folder named after the repository and the commit.
    let root = fs::read_dir(&unpacked)?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .context("The archive of the repository is empty")?;
    let start = if folder.is_empty() {
        root.clone()
    } else {
        root.join(folder)
    };
    ensure!(start.is_dir(), "The repository has no folder {folder}");
    let mut found = Vec::new();
    collect_skills(&start, 0, &mut found);
    found.sort_by_key(|(name, _, _)| name.to_lowercase());
    found.dedup_by(|a, b| a.0 == b.0);
    let web_reference = if reference == "HEAD" {
        "main"
    } else {
        reference
    };
    Ok(found
        .into_iter()
        .map(|(name, description, path)| {
            let relative = path
                .strip_prefix(&root)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            CatalogSkill {
                name,
                description,
                url: format!(
                    "https://github.com/{owner}/{repository}/tree/{web_reference}/{relative}"
                ),
                path: relative,
                folder: path,
                _archive: Some(archive.clone()),
            }
        })
        .collect())
}

/// Lists the skills in a folder on this computer: the folder itself when it holds a `SKILL.md`,
/// else the skill folders under it.
pub fn read_skill_folder(folder: &Path) -> Result<Vec<CatalogSkill>> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    let mut found = Vec::new();
    collect_skills(folder, 0, &mut found);
    ensure!(!found.is_empty(), "The folder holds no SKILL.md");
    found.sort_by_key(|(name, _, _)| name.to_lowercase());
    Ok(found
        .into_iter()
        .map(|(name, description, path)| CatalogSkill {
            name,
            description,
            url: String::new(),
            path: super::display_path(&path),
            folder: path,
            _archive: None,
        })
        .collect())
}

/// The skills.sh directory, which indexes the skills of public GitHub repositories with how
/// often each was installed.
const DIRECTORY: &str = "https://skills.sh";

/// The words whose searches make the directory's list. skills.sh has no listing, only a search
/// that returns the 200 best matches of a query, so the list joins the matches of broad words.
pub const DIRECTORY_SEEDS: [&str; 25] = [
    "skill",
    "agent",
    "code",
    "test",
    "react",
    "python",
    "design",
    "data",
    "docs",
    "git",
    "api",
    "web",
    "security",
    "cloud",
    "database",
    "typescript",
    "ai",
    "writing",
    "review",
    "deploy",
    "mobile",
    "marketing",
    "research",
    "browser",
    "automation",
];

/// A skill of the skills.sh directory.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectorySkill {
    /// `owner/repository/skill`, which names the skill in the directory.
    pub id: String,
    /// The repository on GitHub, as `owner/repository`.
    pub source: String,
    /// The skill's slug in its repository, usually its folder name.
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    /// The description of its `SKILL.md`; absent until it is read, since the directory's search
    /// does not send it.
    #[serde(default)]
    pub description: Option<String>,
}

impl DirectorySkill {
    /// Returns the GitHub account that publishes the skill.
    pub fn owner(&self) -> &str {
        self.source.split('/').next().unwrap_or_default()
    }

    /// Returns the address of the skill's page on skills.sh.
    pub fn url(&self) -> String {
        format!("{DIRECTORY}/{}", self.id)
    }

    /// Reports whether every word, in lower case, is in the skill's name, repository or
    /// description.
    pub fn matches(&self, words: &[String]) -> bool {
        if words.is_empty() {
            return true;
        }
        let haystack = format!(
            "{}\n{}\n{}",
            self.name,
            self.source,
            self.description.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        words.iter().all(|word| haystack.contains(word.as_str()))
    }
}

/// The directory's list as Kodem last read it. The list is kept after each seed word, so a read
/// that stops continues with the next word on a later start.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Directory {
    /// The skills, most installed first.
    pub skills: Vec<DirectorySkill>,
    /// When the list was last complete, in seconds since the Unix epoch; zero until the first
    /// read is complete.
    pub read_at: u64,
    /// How many of `DIRECTORY_SEEDS` an unfinished first read has searched.
    #[serde(default)]
    pub seeds: usize,
}

/// Searches the skills.sh directory. The query takes at least two characters.
pub fn search_directory(query: &str) -> Result<Vec<DirectorySkill>> {
    let query = query.trim();
    ensure!(
        query.chars().count() >= 2,
        "A search takes at least two characters"
    );
    let url = format!(
        "{DIRECTORY}/api/search?q={}&limit=200",
        percent_encode(query)
    );
    let body = crate::harness::fetch(&url).context("skills.sh did not answer")?;
    let document: Value =
        serde_json::from_slice(&body).context("skills.sh sent an unreadable answer")?;
    if let Some(error) = document["error"].as_str() {
        anyhow::bail!("skills.sh: {error}");
    }
    let mut found: Vec<DirectorySkill> = document["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(directory_skill)
        .collect();
    found.sort_by_key(|skill| std::cmp::Reverse(skill.installs));
    Ok(found)
}

// Reads a skill of a directory answer. A skill whose repository is not a GitHub repository
// name is left out, since its installation downloads that repository.
fn directory_skill(entry: &Value) -> Option<DirectorySkill> {
    let source = entry["source"].as_str()?.trim().to_owned();
    let skill_id = entry["skillId"].as_str()?.trim().to_owned();
    let (owner, repository) = source.split_once('/')?;
    if !valid_segment(owner) || !valid_segment(repository) || skill_id.is_empty() {
        return None;
    }
    let name = entry["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&skill_id)
        .to_owned();
    Some(DirectorySkill {
        id: format!("{source}/{skill_id}"),
        source,
        skill_id,
        name,
        installs: entry["installs"].as_u64().unwrap_or_default(),
        description: None,
    })
}

/// Adds skills to a list: a skill that the list holds takes the new install count and keeps
/// its description. The list stays most installed first.
pub fn merge_directory(skills: &mut Vec<DirectorySkill>, found: Vec<DirectorySkill>) {
    let mut index: std::collections::HashMap<String, usize> = skills
        .iter()
        .enumerate()
        .map(|(at, skill)| (skill.id.clone(), at))
        .collect();
    for mut entry in found {
        match index.get(&entry.id) {
            Some(&at) => skills[at].installs = entry.installs,
            None => {
                entry.description = None;
                index.insert(entry.id.clone(), skills.len());
                skills.push(entry);
            }
        }
    }
    skills.sort_by_key(|skill| std::cmp::Reverse(skill.installs));
}

/// Reads the description of a directory skill from the `SKILL.md` that skills.sh keeps of it.
pub fn directory_description(skill: &DirectorySkill) -> Result<String> {
    let url = format!(
        "{DIRECTORY}/api/download/{}/{}",
        skill
            .source
            .split('/')
            .map(percent_encode)
            .collect::<Vec<_>>()
            .join("/"),
        percent_encode(&skill.skill_id)
    );
    let body = crate::harness::fetch(&url).context("skills.sh did not send the skill")?;
    let document: Value =
        serde_json::from_slice(&body).context("skills.sh sent an unreadable skill")?;
    let manifest = document["files"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|file| file["path"].as_str() == Some("SKILL.md"))
        .and_then(|file| file["contents"].as_str())
        .context("skills.sh has no SKILL.md of the skill")?;
    Ok(skills::front_matter(manifest).1)
}

/// Downloads the repository of a directory skill and returns the skill from it, found by its
/// folder name, else by its name.
pub fn read_directory_skill(skill: &DirectorySkill) -> Result<CatalogSkill> {
    let (owner, repository) = skill
        .source
        .split_once('/')
        .context("The skill names no repository")?;
    let mut found = read_skill_repository(owner, repository, "", "")?;
    let folder_is = |candidate: &CatalogSkill| {
        candidate
            .folder
            .file_name()
            .is_some_and(|name| name.to_string_lossy() == skill.skill_id)
    };
    let position = found
        .iter()
        .position(folder_is)
        .or_else(|| {
            found
                .iter()
                .position(|candidate| candidate.name == skill.name)
        })
        .or_else(|| {
            found
                .iter()
                .position(|candidate| candidate.name == skill.skill_id)
        })
        .with_context(|| format!("{} holds no skill named {}", skill.source, skill.name))?;
    Ok(found.swap_remove(position))
}

// Returns the file that keeps the directory's list between starts.
fn directory_cache() -> Result<PathBuf> {
    Ok(kodem_core::ipc::data_dir()?
        .join("plugins")
        .join("skills-directory.json"))
}

/// Returns the directory's list as the last read kept it, when there is one.
pub fn cached_directory() -> Option<Directory> {
    let bytes = fs::read(directory_cache().ok()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Keeps the directory's list for the next start.
pub fn save_directory(directory: &Directory) -> Result<()> {
    let path = directory_cache()?;
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    super::write_atomically(&path, &serde_json::to_vec(directory)?)
}

// Finds the folders under `folder` that hold a `SKILL.md`, to a depth of five. A skill's own
// subfolders are not searched.
fn collect_skills(folder: &Path, depth: usize, found: &mut Vec<(String, String, PathBuf)>) {
    if depth > 5 || found.len() >= 2000 {
        return;
    }
    let manifest = folder.join("SKILL.md");
    if manifest.is_file() {
        let (name, description) = skills::manifest_fields(&manifest);
        let name = name.unwrap_or_else(|| {
            folder
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        found.push((name, description, folder.to_path_buf()));
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_none_or(|name| name != ".git"))
        .collect();
    folders.sort();
    for child in folders {
        collect_skills(&child, depth + 1, found);
    }
}

// Reports whether text can be an owner or a repository name on GitHub.
fn valid_segment(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The parts of a GitHub address: owner, repository, reference and folder. The reference and
/// the folder are empty when the address names the repository only.
pub fn parse_github(address: &str) -> Result<(String, String, String, String)> {
    let address = address.trim().trim_end_matches('/');
    let rest = address
        .strip_prefix("https://github.com/")
        .or_else(|| address.strip_prefix("http://github.com/"))
        .or_else(|| address.strip_prefix("github.com/"))
        .unwrap_or(address);
    let parts: Vec<&str> = rest.split('/').collect();
    ensure!(
        parts.len() >= 2,
        "Enter a GitHub address such as github.com/owner/repository"
    );
    let owner = parts[0].to_owned();
    let repository = parts[1].trim_end_matches(".git").to_owned();
    ensure!(
        valid_segment(&owner) && valid_segment(&repository),
        "This is not the address of a GitHub repository"
    );
    let (reference, folder) = match parts.get(2) {
        Some(&"tree") | Some(&"blob") => {
            let reference = parts.get(3).copied().unwrap_or_default().to_owned();
            let mut folder = parts
                .get(4..)
                .map(|rest| rest.join("/"))
                .unwrap_or_default();
            // An address of a skill's SKILL.md names its folder.
            if let Some(stripped) = folder.strip_suffix("SKILL.md") {
                folder = stripped.trim_end_matches('/').to_owned();
            }
            (reference, folder)
        }
        _ => (String::new(), String::new()),
    };
    Ok((owner, repository, reference, folder))
}

/// A GitHub repository that a web search found.
#[derive(Clone, Debug)]
pub struct WebResult {
    pub name: String,
    pub description: String,
    pub url: String,
    pub stars: u64,
}

/// Searches GitHub's repositories for MCP servers that match `query`, the most starred first.
/// GitHub answers about ten searches a minute without a sign-in.
pub fn search_web(query: &str) -> Result<Vec<WebResult>> {
    let terms = format!("{} mcp server", query.trim());
    let url = format!(
        "https://api.github.com/search/repositories?per_page=20&sort=stars&q={}",
        percent_encode(&terms)
    );
    let body = crate::harness::fetch(&url).context("GitHub's search did not answer")?;
    let document: Value = serde_json::from_slice(&body)?;
    Ok(document["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(WebResult {
                name: item["full_name"].as_str()?.to_owned(),
                description: item["description"].as_str().unwrap_or_default().to_owned(),
                url: item["html_url"].as_str()?.to_owned(),
                stars: item["stargazers_count"].as_u64().unwrap_or_default(),
            })
        })
        .collect())
}

/// Reads a repository's README and returns the MCP configurations in its code blocks: JSON with
/// a command or an address, TOML of `[mcp_servers]`, and `mcp add` commands.
pub fn readme_configurations(repository: &str) -> Result<Vec<mcp::Server>> {
    let url = format!("https://api.github.com/repos/{repository}/readme");
    let body = crate::harness::curl(
        &url,
        "application/vnd.github.raw+json",
        Duration::from_secs(30),
        2 * 1024 * 1024,
    )
    .stderr(Stdio::null())
    .output()
    .context("curl did not start")?;
    ensure!(body.status.success(), "GitHub did not send the README");
    let readme = String::from_utf8_lossy(&body.stdout);
    let mut found: Vec<mcp::Server> = Vec::new();
    for block in code_blocks(&readme) {
        let candidate = block.trim();
        let looks_like_config = candidate.contains("mcpServers")
            || candidate.contains("\"servers\"")
            || candidate.contains("[mcp_servers")
            || candidate.contains("mcp add");
        if !looks_like_config {
            continue;
        }
        // A block of several commands is read a line at a time.
        let pieces: Vec<String> =
            if candidate.starts_with('{') || candidate.contains("[mcp_servers") {
                vec![candidate.to_owned()]
            } else {
                candidate
                    .lines()
                    .map(|line| line.trim().trim_start_matches("$ ").to_owned())
                    .filter(|line| {
                        let mut words = line.split_whitespace();
                        let cli = words.next().unwrap_or_default();
                        matches!(
                            cli,
                            "claude" | "codex" | "grok" | "gemini" | "agy" | "opencode"
                        ) && words.next() == Some("mcp")
                            && words.next() == Some("add")
                    })
                    .collect()
            };
        for piece in pieces {
            if let Ok(servers) = mcp::parse(&piece) {
                for server in servers {
                    // The same server often appears once per client that a README covers.
                    let summary = server.transport.summary();
                    if !found.iter().any(|known| {
                        known.name == server.name && known.transport.summary() == summary
                    }) {
                        found.push(server);
                    }
                }
            }
        }
        if found.len() >= 12 {
            break;
        }
    }
    Ok(found)
}

// Returns the contents of the fenced code blocks of Markdown.
fn code_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        let fence = line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~");
        match (&mut current, fence) {
            (None, true) => current = Some(String::new()),
            (Some(_), true) => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
            }
            (Some(block), false) => {
                block.push_str(line);
                block.push('\n');
            }
            (None, false) => {}
        }
    }
    blocks
}
