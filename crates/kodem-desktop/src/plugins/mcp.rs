//! MCP servers in the configuration of each provider's CLI, for every account of the provider.
//!
//! | CLI | File | Key |
//! | --- | --- | --- |
//! | Claude Code | `~/.claude.json`, or `.claude.json` in an added account's folder | `mcpServers` |
//! | Codex | `config.toml` of `~/.codex` or the account's `CODEX_HOME` | `[mcp_servers.<name>]` |
//! | OpenCode | `~/.config/opencode/opencode.json` or `opencode.jsonc` | `mcp` |
//! | Antigravity | `~/.gemini/config/mcp_config.json` | `mcpServers` |
//! | Grok Build | `config.toml` of `~/.grok` or the account's `GROK_HOME` | `[mcp_servers.<name>]` |
//!
//! A change keeps the keys of an entry that Kodem does not know, such as timeouts and approval
//! rules, and the rest of the file. TOML files keep their comments and layout. A JSON file that
//! holds comments loses them, so a copy of it is kept beside it first.
use super::{account_home, provider_accounts, resolved, write_atomically};
use anyhow::{Context, Result, bail, ensure};
use kodem_core::{Account, Provider, platform};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, InlineTable, Item, Table, TableLike};

/// The name that Kodem gives the MCP server it adds to each session of a task.
pub const RESERVED: &str = "kodem";

/// How a CLI reaches an MCP server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transport {
    /// A program that the CLI starts and talks to over its standard input and output.
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// A remote server over streamable HTTP.
    Http {
        url: String,
        headers: Vec<(String, String)>,
    },
    /// A remote server over the older HTTP and server-sent events transport.
    Sse {
        url: String,
        headers: Vec<(String, String)>,
    },
}

/// The kind of a transport, which decides the fields that a server needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Stdio,
    Http,
    Sse,
}

impl Kind {
    /// Every kind, in the order that the forms offer them.
    pub const ALL: [Self; 3] = [Self::Stdio, Self::Http, Self::Sse];

    /// Returns the kind's short name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stdio => "Local command",
            Self::Http => "Remote HTTP",
            Self::Sse => "Remote SSE",
        }
    }
}

impl Transport {
    /// Returns the kind of the transport.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Stdio { .. } => Kind::Stdio,
            Self::Http { .. } => Kind::Http,
            Self::Sse { .. } => Kind::Sse,
        }
    }

    /// Returns one line that says how the server starts or where it is.
    pub fn summary(&self) -> String {
        match self {
            Self::Stdio { command, args, .. } => {
                let mut line = command.clone();
                for arg in args {
                    line.push(' ');
                    line.push_str(arg);
                }
                line
            }
            Self::Http { url, .. } | Self::Sse { url, .. } => url.clone(),
        }
    }
}

/// An MCP server under the name that the CLIs know it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    pub transport: Transport,
}

/// A server as the configuration of one provider holds it.
#[derive(Clone, Debug)]
pub struct Entry {
    pub provider: Provider,
    pub server: Server,
    /// The CLI starts the server. A CLI without a switch for it always does.
    pub enabled: bool,
    /// The files of the provider's accounts that hold the server.
    pub files: Vec<PathBuf>,
}

/// The servers that every provider's configuration holds, and the files that could not be read.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub entries: Vec<Entry>,
    pub errors: Vec<String>,
}

/// Reports whether a provider's CLI can reach a server of this kind.
pub fn supports(provider: Provider, kind: Kind) -> bool {
    match kind {
        Kind::Stdio | Kind::Http => true,
        // Codex and the Antigravity CLI speak streamable HTTP only.
        Kind::Sse => !matches!(provider, Provider::Codex | Provider::Antigravity),
    }
}

/// Reports whether a provider's configuration can switch a server off without removing it.
/// Claude Code turns servers off per project only.
pub fn switchable(provider: Provider) -> bool {
    provider != Provider::Claude
}

/// Checks a server name against the rules of every CLI: letters, digits, hyphens and
/// underscores, starting with a letter.
pub fn validate_name(name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "Give the server a name");
    ensure!(name.len() <= 64, "Use a name of at most 64 characters");
    ensure!(
        name.starts_with(|c: char| c.is_ascii_alphabetic()),
        "Start the name with a letter"
    );
    ensure!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "Use only letters, digits, hyphens and underscores in the name"
    );
    ensure!(
        !name.eq_ignore_ascii_case(RESERVED),
        "Kodem gives each task a server named kodem; choose another name"
    );
    Ok(())
}

/// Returns a valid server name made from free text, such as the name of a catalog entry like
/// `io.github.owner/some-server`: its last part, in lowercase, with other characters as
/// hyphens.
pub fn suggested_name(text: &str) -> String {
    let last = text.rsplit('/').next().unwrap_or(text);
    let mut name = String::new();
    for c in last.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-').to_owned();
    let name = match name.strip_suffix("-mcp-server") {
        Some(stem) if !stem.is_empty() => stem.to_owned(),
        _ => name,
    };
    let mut name = name
        .trim_start_matches(|c: char| !c.is_ascii_alphabetic())
        .to_owned();
    name.truncate(64);
    if name.is_empty() || name == RESERVED {
        name = "server".into();
    }
    name
}

/// Reads the servers of every provider's configuration, for all accounts of each provider.
pub fn read(accounts: &[Account]) -> Inventory {
    let mut inventory = Inventory::default();
    for provider in Provider::ALL {
        for target in targets(accounts, provider) {
            if !target.path.is_file() {
                continue;
            }
            match target.servers() {
                Ok(servers) => {
                    for (server, enabled) in servers {
                        inventory.add(provider, server, enabled, &target.path);
                    }
                }
                Err(error) => inventory.errors.push(format!(
                    "{}: {}: {error:#}",
                    provider.label(),
                    super::display_path(&target.path)
                )),
            }
        }
    }
    inventory
}

impl Inventory {
    // Adds a server that one file of a provider holds. The files of a provider's accounts that
    // hold the same name are one entry.
    fn add(&mut self, provider: Provider, server: Server, enabled: bool, path: &Path) {
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.provider == provider && entry.server.name == server.name)
        {
            Some(entry) => {
                entry.enabled |= enabled;
                entry.files.push(path.to_path_buf());
            }
            None => self.entries.push(Entry {
                provider,
                server,
                enabled,
                files: vec![path.to_path_buf()],
            }),
        }
    }
}

/// Writes `server` into the configuration of each provider in `providers`, for all accounts of
/// each. An entry of the same name takes the new transport and keeps its other keys. Returns the
/// failures, one per provider.
pub fn install(
    accounts: &[Account],
    providers: &[Provider],
    server: &Server,
) -> Vec<(Provider, String)> {
    let mut failures = Vec::new();
    if let Err(error) = validate_name(&server.name) {
        return providers
            .iter()
            .map(|provider| (*provider, format!("{error:#}")))
            .collect();
    }
    for provider in providers {
        let result = if supports(*provider, server.transport.kind()) {
            targets(accounts, *provider)
                .iter()
                .try_for_each(|target| target.put(server))
        } else {
            Err(anyhow::anyhow!(
                "{} cannot reach a server over {}",
                provider.label(),
                server.transport.kind().label()
            ))
        };
        if let Err(error) = result {
            failures.push((*provider, format!("{error:#}")));
        }
    }
    failures
}

/// Removes the server `name` from the configuration of each provider in `providers`, for all
/// accounts of each. Returns the failures, one per provider.
pub fn remove(accounts: &[Account], providers: &[Provider], name: &str) -> Vec<(Provider, String)> {
    let mut failures = Vec::new();
    for provider in providers {
        let result = targets(accounts, *provider)
            .iter()
            .filter(|target| target.path.is_file())
            .try_for_each(|target| target.delete(name));
        if let Err(error) = result {
            failures.push((*provider, format!("{error:#}")));
        }
    }
    failures
}

/// Switches the server `name` on or off in a provider's configuration, for all its accounts.
pub fn set_enabled(
    accounts: &[Account],
    provider: Provider,
    name: &str,
    enabled: bool,
) -> Result<()> {
    ensure!(
        switchable(provider),
        "{} turns servers off per project only",
        provider.label()
    );
    for target in targets(accounts, provider) {
        if target.path.is_file() {
            target.switch(name, enabled)?;
        }
    }
    Ok(())
}

/// The format of a configuration file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Claude,
    Codex,
    OpenCode,
    Antigravity,
    Grok,
}

/// One configuration file of a provider.
#[derive(Clone, Debug)]
struct Target {
    path: PathBuf,
    format: Format,
}

/// Returns the configuration files of a provider: one for each account whose configuration is
/// its own. Accounts that share a file through a link name it once. An added account's file
/// that does not exist yet is left alone, because the CLI creates it at sign-in.
fn targets(accounts: &[Account], provider: Provider) -> Vec<Target> {
    let mut found: Vec<Target> = Vec::new();
    for account in provider_accounts(accounts, provider) {
        let Some(target) = target_of(&account) else {
            continue;
        };
        if !account.is_default() && !target.path.exists() {
            continue;
        }
        let path = resolved(&target.path);
        if !found.iter().any(|known| resolved(&known.path) == path) {
            found.push(target);
        }
        // Accounts of OpenCode and Antigravity share one configuration.
        if matches!(provider, Provider::OpenCode | Provider::Antigravity) {
            break;
        }
    }
    found
}

// Returns the configuration file of one account.
fn target_of(account: &Account) -> Option<Target> {
    let home = account_home(account).ok()?;
    let (path, format) = match account.provider {
        Provider::Claude => (
            match &account.home {
                Some(home) => home.join(".claude.json"),
                None => platform::home_dir().ok()?.join(".claude.json"),
            },
            Format::Claude,
        ),
        Provider::Codex => (home.join("config.toml"), Format::Codex),
        Provider::OpenCode => (opencode_file(&home), Format::OpenCode),
        Provider::Antigravity => (home.join("mcp_config.json"), Format::Antigravity),
        Provider::Grok => (home.join("config.toml"), Format::Grok),
    };
    Some(Target { path, format })
}

// Returns OpenCode's configuration file in its folder: the one that exists, else a new
// `opencode.json`.
fn opencode_file(folder: &Path) -> PathBuf {
    ["opencode.jsonc", "opencode.json", "config.json"]
        .into_iter()
        .map(|name| folder.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| folder.join("opencode.json"))
}

impl Target {
    fn toml(&self) -> bool {
        matches!(self.format, Format::Codex | Format::Grok)
    }

    // The key of the file's object that holds the servers.
    fn json_key(&self) -> &'static str {
        match self.format {
            Format::OpenCode => "mcp",
            _ => "mcpServers",
        }
    }

    // Reads the servers of the file, each with whether the CLI starts it.
    fn servers(&self) -> Result<Vec<(Server, bool)>> {
        let mut servers = Vec::new();
        if self.toml() {
            let document = read_toml(&self.path)?;
            let disabled = string_list(document.get("disabled_mcp_servers"));
            let Some(table) = document.get("mcp_servers").and_then(Item::as_table_like) else {
                return Ok(servers);
            };
            for (name, item) in table.iter() {
                let Some(entry) = item.as_table_like() else {
                    continue;
                };
                if let Some(transport) = toml_transport(entry, self.format) {
                    let enabled = entry.get("enabled").and_then(Item::as_bool).unwrap_or(true)
                        && !disabled.iter().any(|known| known == name);
                    servers.push((
                        Server {
                            name: name.to_owned(),
                            transport,
                        },
                        enabled,
                    ));
                }
            }
        } else {
            let (document, _) = read_json(&self.path)?;
            let Some(table) = document.get(self.json_key()).and_then(Value::as_object) else {
                return Ok(servers);
            };
            for (name, entry) in table {
                let Some(transport) = json_transport(entry, self.format) else {
                    continue;
                };
                let enabled = match self.format {
                    Format::OpenCode => entry["enabled"].as_bool().unwrap_or(true),
                    Format::Antigravity => entry["disabled"].as_bool() != Some(true),
                    _ => true,
                };
                servers.push((
                    Server {
                        name: name.clone(),
                        transport,
                    },
                    enabled,
                ));
            }
        }
        Ok(servers)
    }

    // Writes a server into the file, creating the file when it does not exist.
    fn put(&self, server: &Server) -> Result<()> {
        if self.toml() {
            let mut document = read_toml(&self.path)?;
            let servers = toml_section(&mut document)?;
            if !servers
                .get(&server.name)
                .is_some_and(|item| item.as_table_like().is_some())
            {
                servers.insert(&server.name, Item::Table(Table::new()));
            }
            let entry = servers
                .get_mut(&server.name)
                .and_then(Item::as_table_like_mut)
                .context("The server's entry is not a table")?;
            put_toml(entry, &server.transport, self.format);
            return write_atomically(&self.path, document.to_string().as_bytes());
        }
        let (mut document, commented) = read_json(&self.path)?;
        let object = document
            .as_object_mut()
            .context("The file does not hold a JSON object")?;
        if self.format == Format::OpenCode && object.is_empty() {
            object.insert("$schema".into(), json!("https://opencode.ai/config.json"));
        }
        let servers = object
            .entry(self.json_key())
            .or_insert_with(|| Value::Object(Map::new()));
        let servers = servers
            .as_object_mut()
            .with_context(|| format!("`{}` is not a JSON object", self.json_key()))?;
        let entry = servers
            .entry(server.name.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Some(entry) = entry.as_object_mut() {
            put_json(entry, &server.transport, self.format);
        }
        self.write_json(&document, commented)
    }

    // Removes a server from the file.
    fn delete(&self, name: &str) -> Result<()> {
        if self.toml() {
            let mut document = read_toml(&self.path)?;
            let removed = document
                .get_mut("mcp_servers")
                .and_then(Item::as_table_like_mut)
                .and_then(|servers| servers.remove(name))
                .is_some();
            let listed = remove_from_list(&mut document, "disabled_mcp_servers", name);
            if removed || listed {
                write_atomically(&self.path, document.to_string().as_bytes())?;
            }
            return Ok(());
        }
        let (mut document, commented) = read_json(&self.path)?;
        let removed = document
            .get_mut(self.json_key())
            .and_then(Value::as_object_mut)
            .and_then(|servers| servers.shift_remove(name))
            .is_some();
        if removed {
            self.write_json(&document, commented)?;
        }
        Ok(())
    }

    // Switches a server on or off. A file that does not hold the server is left alone.
    fn switch(&self, name: &str, enabled: bool) -> Result<()> {
        if self.toml() {
            let mut document = read_toml(&self.path)?;
            let Some(entry) = document
                .get_mut("mcp_servers")
                .and_then(Item::as_table_like_mut)
                .and_then(|servers| servers.get_mut(name))
                .and_then(Item::as_table_like_mut)
            else {
                return Ok(());
            };
            if enabled {
                entry.remove("enabled");
            } else {
                entry.insert("enabled", toml_edit::value(false));
            }
            // Grok also skips the servers that its list names.
            if enabled {
                remove_from_list(&mut document, "disabled_mcp_servers", name);
            }
            return write_atomically(&self.path, document.to_string().as_bytes());
        }
        let (mut document, commented) = read_json(&self.path)?;
        let Some(entry) = document
            .get_mut(self.json_key())
            .and_then(Value::as_object_mut)
            .and_then(|servers| servers.get_mut(name))
            .and_then(Value::as_object_mut)
        else {
            return Ok(());
        };
        match self.format {
            Format::OpenCode => {
                entry.insert("enabled".into(), json!(enabled));
            }
            Format::Antigravity if enabled => {
                entry.shift_remove("disabled");
            }
            Format::Antigravity => {
                entry.insert("disabled".into(), json!(true));
            }
            _ => bail!("This configuration has no switch for a server"),
        }
        self.write_json(&document, commented)
    }

    // Writes the JSON document over the file. A file with comments is copied beside itself
    // first, because the document no longer holds them.
    fn write_json(&self, document: &Value, commented: bool) -> Result<()> {
        if commented {
            let mut backup = self.path.clone().into_os_string();
            backup.push(".kodem-backup");
            fs::copy(&self.path, PathBuf::from(backup))
                .context("Cannot keep a copy of the file with its comments")?;
        }
        let mut text = serde_json::to_string_pretty(document)?;
        text.push('\n');
        write_atomically(&self.path, text.as_bytes())
    }
}

// Reads a TOML file; a missing file is an empty document.
fn read_toml(path: &Path) -> Result<DocumentMut> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    text.parse::<DocumentMut>()
        .context("The file is not valid TOML")
}

// Returns the `mcp_servers` table of a TOML document, created when it is missing. A new table
// has no header of its own: each server gets `[mcp_servers.<name>]`.
fn toml_section(document: &mut DocumentMut) -> Result<&mut dyn TableLike> {
    if document
        .get("mcp_servers")
        .is_none_or(|item| item.as_table_like().is_none())
    {
        let mut table = Table::new();
        table.set_implicit(true);
        document.insert("mcp_servers", Item::Table(table));
    }
    document
        .get_mut("mcp_servers")
        .and_then(Item::as_table_like_mut)
        .context("`mcp_servers` is not a table")
}

// Reads the transport of a TOML entry of Codex or Grok.
fn toml_transport(entry: &dyn TableLike, format: Format) -> Option<Transport> {
    let text = |key: &str| entry.get(key).and_then(Item::as_str).map(str::to_owned);
    if let Some(url) = text("url") {
        let header_key = if format == Format::Codex {
            "http_headers"
        } else {
            "headers"
        };
        let headers = string_table(entry.get(header_key));
        return Some(if text("type").as_deref() == Some("sse") {
            Transport::Sse { url, headers }
        } else {
            Transport::Http { url, headers }
        });
    }
    Some(Transport::Stdio {
        command: text("command")?,
        args: string_list(entry.get("args")),
        env: string_table(entry.get("env")),
    })
}

// Writes a transport into a TOML entry of Codex or Grok and removes the keys of the other
// kinds. Other keys stay.
fn put_toml(entry: &mut dyn TableLike, transport: &Transport, format: Format) {
    let header_key = if format == Format::Codex {
        "http_headers"
    } else {
        "headers"
    };
    for key in ["command", "args", "env", "url", header_key] {
        entry.remove(key);
    }
    if format == Format::Grok {
        entry.remove("type");
    }
    match transport {
        Transport::Stdio { command, args, env } => {
            entry.insert("command", toml_edit::value(command.as_str()));
            if !args.is_empty() {
                entry.insert(
                    "args",
                    toml_edit::value(toml_edit::Array::from_iter(args.iter().map(String::as_str))),
                );
            }
            if !env.is_empty() {
                entry.insert("env", toml_edit::value(inline_table(env)));
            }
        }
        Transport::Http { url, headers } | Transport::Sse { url, headers } => {
            if format == Format::Grok && transport.kind() == Kind::Sse {
                entry.insert("type", toml_edit::value("sse"));
            }
            entry.insert("url", toml_edit::value(url.as_str()));
            if !headers.is_empty() {
                entry.insert(header_key, toml_edit::value(inline_table(headers)));
            }
        }
    }
}

// Returns pairs as an inline TOML table.
fn inline_table(pairs: &[(String, String)]) -> InlineTable {
    let mut table = InlineTable::new();
    for (key, value) in pairs {
        table.insert(key, value.as_str().into());
    }
    table
}

// Reads an array of strings.
fn string_list(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// Reads a table of strings, in its order.
fn string_table(item: Option<&Item>) -> Vec<(String, String)> {
    item.and_then(Item::as_table_like)
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, value)| Some((key.to_owned(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

// Removes `name` from an array of strings at the top of a TOML document. Reports whether it
// was there.
fn remove_from_list(document: &mut DocumentMut, key: &str, name: &str) -> bool {
    let Some(list) = document.get_mut(key).and_then(Item::as_array_mut) else {
        return false;
    };
    let before = list.len();
    list.retain(|value| value.as_str() != Some(name));
    list.len() != before
}

// Reads a JSON file that may hold comments and trailing commas, as OpenCode's `.jsonc` does. A
// missing or empty file is an empty object. Also reports whether the file held comments.
fn read_json(path: &Path) -> Result<(Value, bool)> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    if text.trim().is_empty() {
        return Ok((Value::Object(Map::new()), false));
    }
    let (plain, commented) = strip_comments(&text);
    let value: Value = serde_json::from_str(&plain).context("The file is not valid JSON")?;
    ensure!(value.is_object(), "The file does not hold a JSON object");
    Ok((value, commented))
}

/// Removes the comments and trailing commas of JSON with comments, outside of strings. Also
/// reports whether there were comments.
pub fn strip_comments(text: &str) -> (String, bool) {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut commented = false;
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            plain.push(c);
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    plain.push(escaped);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                plain.push(c);
            }
            ('/', Some('/')) => {
                commented = true;
                for next in chars.by_ref() {
                    if next == '\n' {
                        plain.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                commented = true;
                chars.next();
                let mut last = '\0';
                for next in chars.by_ref() {
                    if last == '*' && next == '/' {
                        break;
                    }
                    last = next;
                }
            }
            _ => plain.push(c),
        }
    }
    // A comma that only whitespace separates from a closing bracket is dropped.
    let mut result = String::with_capacity(plain.len());
    let mut in_string = false;
    let mut escaped = false;
    let characters: Vec<char> = plain.chars().collect();
    for (index, c) in characters.iter().enumerate() {
        if in_string {
            result.push(*c);
            if escaped {
                escaped = false;
            } else if *c == '\\' {
                escaped = true;
            } else if *c == '"' {
                in_string = false;
            }
            continue;
        }
        if *c == '"' {
            in_string = true;
        }
        if *c == ','
            && characters[index + 1..]
                .iter()
                .find(|next| !next.is_whitespace())
                .is_some_and(|next| matches!(next, '}' | ']'))
        {
            continue;
        }
        result.push(*c);
    }
    (result, commented)
}

// Reads the transport of a JSON entry of Claude Code, OpenCode or Antigravity.
fn json_transport(entry: &Value, format: Format) -> Option<Transport> {
    let text = |key: &str| entry[key].as_str().map(str::to_owned);
    let pairs = |key: &str| -> Vec<(String, String)> {
        entry[key]
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default()
    };
    match format {
        Format::OpenCode => match entry["type"].as_str() {
            Some("remote") => Some(Transport::Http {
                url: text("url")?,
                headers: pairs("headers"),
            }),
            _ => {
                let mut parts = entry["command"]
                    .as_array()?
                    .iter()
                    .filter_map(|part| part.as_str().map(str::to_owned));
                Some(Transport::Stdio {
                    command: parts.next()?,
                    args: parts.collect(),
                    env: pairs("environment"),
                })
            }
        },
        _ => {
            let url = text("url").or_else(|| text("serverUrl"));
            match (entry["type"].as_str(), url) {
                (Some("sse"), Some(url)) => Some(Transport::Sse {
                    url,
                    headers: pairs("headers"),
                }),
                (Some("http" | "streamable-http"), Some(url)) | (None, Some(url))
                    if entry.get("command").is_none() =>
                {
                    Some(Transport::Http {
                        url,
                        headers: pairs("headers"),
                    })
                }
                _ => Some(Transport::Stdio {
                    command: text("command")?,
                    args: entry["args"]
                        .as_array()
                        .map(|args| {
                            args.iter()
                                .filter_map(|arg| arg.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                    env: pairs("env"),
                }),
            }
        }
    }
}

// Returns pairs as a JSON object, in their order.
fn json_pairs(pairs: &[(String, String)]) -> Value {
    Value::Object(
        pairs
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    )
}

// Writes a transport into a JSON entry and removes the keys of the other kinds. Other keys
// stay.
fn put_json(entry: &mut Map<String, Value>, transport: &Transport, format: Format) {
    for key in [
        "type",
        "command",
        "args",
        "env",
        "environment",
        "url",
        "serverUrl",
        "headers",
    ] {
        entry.shift_remove(key);
    }
    match (format, transport) {
        (Format::OpenCode, Transport::Stdio { command, args, env }) => {
            entry.insert("type".into(), json!("local"));
            let mut parts = vec![command.clone()];
            parts.extend(args.iter().cloned());
            entry.insert("command".into(), json!(parts));
            if !env.is_empty() {
                entry.insert("environment".into(), json_pairs(env));
            }
        }
        (Format::OpenCode, Transport::Http { url, headers } | Transport::Sse { url, headers }) => {
            entry.insert("type".into(), json!("remote"));
            entry.insert("url".into(), json!(url));
            if !headers.is_empty() {
                entry.insert("headers".into(), json_pairs(headers));
            }
        }
        (_, Transport::Stdio { command, args, env }) => {
            if format == Format::Claude {
                entry.insert("type".into(), json!("stdio"));
            }
            entry.insert("command".into(), json!(command));
            entry.insert("args".into(), json!(args));
            if !env.is_empty() {
                entry.insert("env".into(), json_pairs(env));
            }
        }
        (_, Transport::Http { url, headers } | Transport::Sse { url, headers }) => {
            if format == Format::Claude {
                let kind = if transport.kind() == Kind::Sse {
                    "sse"
                } else {
                    "http"
                };
                entry.insert("type".into(), json!(kind));
            }
            entry.insert("url".into(), json!(url));
            if !headers.is_empty() {
                entry.insert("headers".into(), json_pairs(headers));
            }
        }
    }
    // A new OpenCode entry is on; an existing one keeps its switch.
    if format == Format::OpenCode && !entry.contains_key("enabled") {
        entry.insert("enabled".into(), json!(true));
    }
}

/// Reads the servers of a configuration that the user pasted: an `mcpServers`, `servers` or
/// `mcp` object of JSON, one server's JSON entry, `[mcp_servers.<name>]` tables of TOML, a
/// `claude mcp add`, `codex mcp add` or `grok mcp add` command, a command that starts a server,
/// or the address of a remote server.
pub fn parse(text: &str) -> Result<Vec<Server>> {
    let text = text.trim();
    ensure!(!text.is_empty(), "Paste a configuration first");
    if text.starts_with('{') {
        return parse_json(text);
    }
    if text.starts_with('[') || text.contains("[mcp_servers") {
        let document = text
            .parse::<DocumentMut>()
            .context("The configuration is not valid TOML")?;
        let table = document
            .get("mcp_servers")
            .and_then(Item::as_table_like)
            .context("The TOML has no [mcp_servers.<name>] table")?;
        let servers: Vec<Server> = table
            .iter()
            .filter_map(|(name, item)| {
                let transport = toml_transport(item.as_table_like()?, Format::Grok)
                    .or_else(|| toml_transport(item.as_table_like()?, Format::Codex))?;
                Some(Server {
                    name: suggested_name(name),
                    transport,
                })
            })
            .collect();
        ensure!(!servers.is_empty(), "The TOML holds no server");
        return Ok(servers);
    }
    if (text.starts_with("https://") || text.starts_with("http://")) && !text.contains(' ') {
        return Ok(vec![Server {
            name: suggested_name(url_stem(text)),
            transport: Transport::Http {
                url: text.into(),
                headers: Vec::new(),
            },
        }]);
    }
    let words = shell_words(text);
    ensure!(!words.is_empty(), "The configuration holds no command");
    if words.len() >= 3 && words[1] == "mcp" && words[2] == "add" {
        return parse_add_command(&words[3..]).map(|server| vec![server]);
    }
    let (env, command) = leading_assignments(&words);
    let mut parts = command.iter().cloned();
    let program = parts.next().context("The configuration holds no command")?;
    let args: Vec<String> = parts.collect();
    Ok(vec![Server {
        name: suggested_name(&package_of(&program, &args)),
        transport: Transport::Stdio {
            command: program,
            args,
            env,
        },
    }])
}

// Reads servers from pasted JSON.
fn parse_json(text: &str) -> Result<Vec<Server>> {
    let (plain, _) = strip_comments(text);
    let value: Value =
        serde_json::from_str(&plain).context("The configuration is not valid JSON")?;
    let object = value.as_object().context("The JSON is not an object")?;
    // An object that holds the servers under a known key, or the servers themselves.
    let (servers, format) = if let Some(servers) =
        object.get("mcpServers").and_then(Value::as_object)
    {
        (servers.clone(), Format::Claude)
    } else if let Some(servers) = object.get("servers").and_then(Value::as_object) {
        (servers.clone(), Format::Claude)
    } else if let Some(servers) = object.get("mcp").and_then(Value::as_object) {
        (servers.clone(), Format::OpenCode)
    } else if object.contains_key("command")
        || object.contains_key("url")
        || object.contains_key("serverUrl")
    {
        let format = if object.get("command").is_some_and(Value::is_array) {
            Format::OpenCode
        } else {
            Format::Claude
        };
        let transport =
            json_transport(&value, format).context("The entry has no command or address")?;
        let name = match &transport {
            Transport::Stdio { command, args, .. } => package_of(command, args),
            Transport::Http { url, .. } | Transport::Sse { url, .. } => url_stem(url).to_owned(),
        };
        return Ok(vec![Server {
            name: suggested_name(&name),
            transport,
        }]);
    } else {
        (object.clone(), Format::Claude)
    };
    let mut found = Vec::new();
    for (name, entry) in &servers {
        let format = if entry.get("command").is_some_and(Value::is_array)
            || entry["type"]
                .as_str()
                .is_some_and(|kind| kind == "local" || kind == "remote")
        {
            Format::OpenCode
        } else {
            format
        };
        if let Some(transport) = json_transport(entry, format) {
            found.push(Server {
                name: suggested_name(name),
                transport,
            });
        }
    }
    ensure!(
        !found.is_empty(),
        "The JSON holds no server with a command or an address"
    );
    Ok(found)
}

// Reads the arguments of `<cli> mcp add`, after `add`: options, the name, then the address or,
// after `--` or directly, the command.
fn parse_add_command(words: &[String]) -> Result<Server> {
    let mut kind = Kind::Stdio;
    let mut env = Vec::new();
    let mut headers = Vec::new();
    let mut rest = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let word = &words[index];
        let value = words.get(index + 1).cloned().unwrap_or_default();
        match word.as_str() {
            "--" => {
                rest.extend(words[index + 1..].iter().cloned());
                break;
            }
            "-t" | "--transport" => {
                kind = match value.as_str() {
                    "http" | "streamable-http" => Kind::Http,
                    "sse" => Kind::Sse,
                    _ => Kind::Stdio,
                };
                index += 2;
            }
            "-e" | "--env" => {
                if let Some((key, val)) = value.split_once('=') {
                    env.push((key.to_owned(), val.to_owned()));
                }
                index += 2;
            }
            "-H" | "--header" => {
                if let Some((key, val)) = value.split_once(':') {
                    headers.push((key.trim().to_owned(), val.trim().to_owned()));
                }
                index += 2;
            }
            "-s" | "--scope" => index += 2,
            "--url" => {
                kind = if kind == Kind::Sse {
                    Kind::Sse
                } else {
                    Kind::Http
                };
                rest.push(value);
                index += 2;
            }
            _ if word.starts_with("--transport=") => {
                kind = match &word["--transport=".len()..] {
                    "http" | "streamable-http" => Kind::Http,
                    "sse" => Kind::Sse,
                    _ => Kind::Stdio,
                };
                index += 1;
            }
            _ if word.starts_with('-') && rest.is_empty() => index += 1,
            _ => {
                rest.push(word.clone());
                index += 1;
            }
        }
    }
    let mut rest = rest.into_iter();
    let name = rest.next().context("The command names no server")?;
    let first = rest
        .next()
        .context("The command has no address or command for the server")?;
    if kind == Kind::Stdio && (first.starts_with("https://") || first.starts_with("http://")) {
        kind = Kind::Http;
    }
    let transport = match kind {
        Kind::Http => Transport::Http {
            url: first,
            headers,
        },
        Kind::Sse => Transport::Sse {
            url: first,
            headers,
        },
        Kind::Stdio => Transport::Stdio {
            command: first,
            args: rest.collect(),
            env,
        },
    };
    Ok(Server {
        name: suggested_name(&name),
        transport,
    })
}

/// Splits a command line into its words, as a POSIX shell reads its quotes.
pub fn command_words(text: &str) -> Vec<String> {
    shell_words(text)
}

// Splits a command line into words, with single and double quotes and backslashes as a POSIX
// shell reads them. Line continuations join lines.
fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            (Some(_), c) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, '\\') => {
                match chars.next() {
                    // A continued line.
                    Some('\n') | None => {}
                    Some(next) => {
                        word.push(next);
                        started = true;
                    }
                }
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if started || !word.is_empty() {
        words.push(word);
    }
    words
}

// Splits `KEY=value` words at the start of a command line from the command.
fn leading_assignments(words: &[String]) -> (Vec<(String, String)>, &[String]) {
    let mut env = Vec::new();
    let mut index = 0;
    while let Some(word) = words.get(index) {
        let Some((key, value)) = word.split_once('=') else {
            break;
        };
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            break;
        }
        env.push((key.to_owned(), value.to_owned()));
        index += 1;
    }
    (env, &words[index..])
}

// Returns the package that a command starts, such as `@scope/server` of `npx -y @scope/server`,
// to name the server after it.
fn package_of(command: &str, args: &[String]) -> String {
    let program = Path::new(command)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| command.to_owned());
    let runner = matches!(
        program.as_str(),
        "npx"
            | "bunx"
            | "pnpx"
            | "uvx"
            | "pipx"
            | "docker"
            | "deno"
            | "node"
            | "python"
            | "python3"
            | "uv"
    );
    // The first word that is not an option, an option's value or a runner's subcommand.
    let mut words = args.iter();
    while runner && let Some(word) = words.next() {
        if matches!(
            word.as_str(),
            "-e" | "--env" | "-v" | "--volume" | "--name" | "--from"
        ) {
            words.next();
            continue;
        }
        if word.starts_with('-') || matches!(word.as_str(), "run" | "dlx" | "exec" | "x" | "tool") {
            continue;
        }
        let package = word
            .rsplit_once('@')
            .filter(|(head, _)| !head.is_empty())
            .map_or(word.as_str(), |(head, _)| head);
        // An image's tag and a Python requirement's version are not part of its name.
        let package = package.split([':', '=']).next().unwrap_or(package);
        return package.to_owned();
    }
    program
}

// Returns the part of an address that names its server: the host without `www.`, `mcp.` or
// `api.`, and without its domain.
fn url_stem(url: &str) -> &str {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', ':', '?'])
        .next()
        .unwrap_or(url);
    let host = host
        .trim_start_matches("www.")
        .trim_start_matches("mcp.")
        .trim_start_matches("api.");
    host.split('.').next().unwrap_or(host)
}
