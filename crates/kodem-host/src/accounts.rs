//! Provider accounts: registry, isolated configuration directories, sign-in and observed status.
//!
//! Every added account owns one CLI configuration directory, so several identities of the same
//! provider stay signed in at once. Kodem only launches the provider's own sign-in and reads the
//! status its CLI reports. It never reads, copies or links credentials.
use crate::{Host, Shared, provider};
use anyhow::{Context, Result, bail, ensure};
use kodem_core::*;
use std::{
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

/// Non-secret Claude Code configuration shared with added accounts. `projects` holds session
/// transcripts; sharing it lets a task resume its session under another account.
const SHARED_CLAUDE: [&str; 9] = [
    "settings.json",
    "CLAUDE.md",
    "skills",
    "commands",
    "agents",
    "plugins",
    "output-styles",
    "keybindings.json",
    "projects",
];
/// Non-secret Codex configuration shared with added accounts. Thread history lives in per-home
/// SQLite state that is deliberately not shared, so `sessions` only exposes CLI rollouts.
const SHARED_CODEX: [&str; 6] = [
    "config.toml",
    "AGENTS.md",
    "prompts",
    "skills",
    "rules",
    "sessions",
];
const SIGN_IN_WAIT: Duration = Duration::from_secs(300);
/// Account quotas reported by a running session stay fresh without another CLI start.
const USAGE_FRESH_MS: u64 = 4 * 60 * 1000;
/// How long an account that no task uses is left alone by the background cadence.
///
/// Kodem contacts a provider for an account only when the account is in use or the user asks.
/// Regular requests for every signed-in account do not look like one person's ordinary use.
const IDLE_CHECK_MS: u64 = 30 * 60 * 1000;
/// How long after its last change a task still counts as using its account.
const RECENT_TASK_MS: u64 = 15 * 60 * 1000;

/// Registers the default account of every provider so existing sign-ins work without setup.
pub(super) fn ensure_defaults(store: &storage::Store) -> Result<()> {
    let mut accounts = store.settings()?.accounts;
    let mut changed = false;
    for provider in Provider::ALL {
        if !accounts
            .iter()
            .any(|account| account.provider == provider && account.is_default())
        {
            accounts.push(Account {
                id: DEFAULT_ACCOUNT.into(),
                provider,
                name: provider.label().into(),
                home: None,
                color: None,
                created: now(),
            });
            changed = true;
        }
    }
    if changed {
        store.save_accounts(accounts)?;
    }
    Ok(())
}

/// Finds an account record; an absent identity selects the provider's default account.
pub(crate) fn resolve(host: &Host, provider: Provider, id: Option<&str>) -> Result<Account> {
    let id = id.unwrap_or(DEFAULT_ACCOUNT);
    host.store
        .settings()?
        .accounts
        .into_iter()
        .find(|account| account.provider == provider && account.id == id)
        .with_context(|| {
            format!(
                "This account was removed or does not belong to {}. Choose another account.",
                provider.label()
            )
        })
}

/// Returns the account that runs this task's turns, never substituting another identity.
pub(crate) fn for_task(host: &Host, task: &Task) -> Result<Account> {
    resolve(host, task.provider, task.account.as_deref())
}

// A reached limit ends when its window resets, even before the provider reports again.
fn clear_expired_limit(usage: &mut UsageLimits) {
    let seconds = now() / 1000;
    let mut exhausted = [&usage.primary, &usage.secondary]
        .into_iter()
        .flatten()
        .filter(|window| window.used_percent >= 100)
        .peekable();
    if usage.rate_limit_reached_type.is_some()
        && exhausted.peek().is_some()
        && exhausted.all(|window| window.resets_at.is_some_and(|reset| reset <= seconds))
    {
        usage.rate_limit_reached_type = None;
    }
}

/// Returns every registered account with its last observed status.
pub(super) fn states(host: &Host) -> Result<Vec<AccountState>> {
    Ok(host
        .store
        .settings()?
        .accounts
        .into_iter()
        .map(|account| {
            let mut status = host
                .accounts
                .get(&(account.provider, account.id.clone()))
                .cloned()
                .unwrap_or_default();
            if let Some(usage) = &mut status.usage {
                clear_expired_limit(usage);
            }
            AccountState { account, status }
        })
        .collect())
}

/// Replaces an account's observed status, keeping a pending sign-in and still valid quotas.
pub(crate) fn observe(host: &mut Host, account: &Account, observed: AccountStatus) {
    let entry = host
        .accounts
        .entry((account.provider, account.id.clone()))
        .or_default();
    let usage = if observed.signed_in == Some(false) {
        None
    } else {
        observed.usage.clone().or(entry.usage.take())
    };
    *entry = AccountStatus {
        usage,
        signing_in: entry.signing_in,
        checked: now(),
        ..observed
    };
}

/// Stores quotas reported for one account by a session or an explicit read.
pub(crate) fn set_usage(host: &mut Host, provider: Provider, id: &str, usage: UsageLimits) {
    host.accounts
        .entry((provider, id.to_owned()))
        .or_default()
        .usage = Some(usage);
}

/// Gives a provider driver the stored quotas of one account for sparse updates.
pub(crate) fn usage_mut<'a>(
    host: &'a mut Host,
    provider: Provider,
    id: &str,
) -> &'a mut Option<UsageLimits> {
    &mut host
        .accounts
        .entry((provider, id.to_owned()))
        .or_default()
        .usage
}

/// Runs a short CLI command to completion, stopping it when it exceeds the time limit.
pub(crate) fn output(mut command: Command, limit: Duration) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().context("Missing command output")?;
    let mut stderr = child.stderr.take().context("Missing command diagnostics")?;
    let read_stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.by_ref().take(1024 * 1024).read_to_end(&mut bytes);
        bytes
    });
    let read_stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.by_ref().take(64 * 1024).read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("The provider CLI did not answer in time");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    Ok(Output {
        status,
        stdout: read_stdout.join().unwrap_or_default(),
        stderr: read_stderr.join().unwrap_or_default(),
    })
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    ensure!(
        !name.is_empty() && name.chars().count() < 80 && !name.chars().any(char::is_control),
        "Enter an account name under 80 characters without control characters"
    );
    Ok(name.to_owned())
}

fn accounts_root(root: &Path) -> PathBuf {
    root.join("accounts")
}

fn standard_home(provider: Provider) -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is unavailable")?);
    Ok(match provider {
        Provider::Claude => home.join(".claude"),
        Provider::Codex => home.join(".codex"),
    })
}

/// Links shared configuration into an account directory without touching anything that exists.
///
/// Only the listed names are linked. Identity and token files are never among them, so two
/// directories can never hold the same credential.
fn link_shared(provider: Provider, home: &Path) -> Result<()> {
    let source = standard_home(provider)?;
    let names: &[&str] = match provider {
        Provider::Claude => &SHARED_CLAUDE,
        Provider::Codex => &SHARED_CODEX,
    };
    for name in names {
        let from = source.join(name);
        // Sessions must be shared even before the standard directory has stored one.
        if provider == Provider::Claude && *name == "projects" && !from.exists() {
            fs::create_dir_all(&from)?;
        }
        let to = home.join(name);
        if !from.exists() || fs::symlink_metadata(&to).is_ok() {
            continue;
        }
        std::os::unix::fs::symlink(&from, &to)
            .with_context(|| format!("Could not share {name} with the new account"))?;
    }
    Ok(())
}

/// Registers another account of a provider with its own configuration directory.
pub(super) fn add(host: &Shared, root: &Path, provider: Provider, name: &str) -> Result<Account> {
    let name = validate_name(name)?;
    ensure!(
        provider.home_variable().is_some(),
        "{} manages a single account through its own CLI. Sign in or out from its default account.",
        provider.label()
    );
    let id = new_id();
    let directory = accounts_root(root)
        .join(format!("{provider:?}").to_lowercase())
        .join(&id);
    fs::create_dir_all(&directory)?;
    for path in [
        accounts_root(root),
        directory.parent().unwrap().to_owned(),
        directory.clone(),
    ] {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    let account = Account {
        id,
        provider,
        name,
        home: Some(directory.clone()),
        color: None,
        created: now(),
    };
    let registered = link_shared(provider, &directory).and_then(|()| {
        let h = host.lock().unwrap();
        let mut accounts = h.store.settings()?.accounts;
        ensure!(
            accounts.len() < 64,
            "Remove an account before adding another one"
        );
        accounts.push(account.clone());
        h.store.save_accounts(accounts)
    });
    if let Err(error) = registered {
        let _ = remove_directory(root, &directory);
        return Err(error);
    }
    let mut h = host.lock().unwrap();
    observe(
        &mut h,
        &account,
        AccountStatus {
            signed_in: Some(false),
            ..AccountStatus::default()
        },
    );
    Ok(account)
}

/// Renames an account or changes its color without touching its sign-in.
pub(super) fn edit(
    host: &Shared,
    provider: Provider,
    id: &str,
    name: &str,
    color: Option<AccentColor>,
) -> Result<()> {
    let name = validate_name(name)?;
    let h = host.lock().unwrap();
    let mut accounts = h.store.settings()?.accounts;
    let account = accounts
        .iter_mut()
        .find(|account| account.provider == provider && account.id == id)
        .context("Account not found")?;
    account.name = name;
    account.color = color;
    h.store.save_accounts(accounts)
}

fn tasks_of(host: &Host, account: &Account) -> Result<Vec<Task>> {
    Ok(host
        .store
        .tasks()?
        .into_iter()
        .filter(|task| task.provider == account.provider && task.account_id() == account.id)
        .collect())
}

fn ensure_idle(host: &Host, account: &Account, action: &str) -> Result<()> {
    for task in tasks_of(host, account)? {
        ensure!(
            !task.state.active()
                && task.active_turn.is_none()
                && task.state != RunState::DeliveryUnknown
                && !host.store.has_uncertain_submission(&task.id)?,
            "Finish or reconcile the tasks running under this account before {action}"
        );
    }
    Ok(())
}

// Deletes only a directory Kodem created for an account. Links are removed, never followed.
fn remove_directory(root: &Path, directory: &Path) -> Result<()> {
    let base = accounts_root(root).canonicalize()?;
    let Ok(target) = directory.canonicalize() else {
        return Ok(());
    };
    ensure!(
        target != base && target.starts_with(&base),
        "This account directory is outside Kodem's data directory and was left in place"
    );
    for entry in fs::read_dir(&target)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            fs::remove_file(entry.path())?;
        }
    }
    fs::remove_dir_all(&target)?;
    Ok(())
}

/// Forgets an added account, signs it out and deletes its configuration directory.
///
/// Tasks that used it keep their reference and are blocked until another account is chosen, so
/// a removed identity is never silently replaced by another one.
pub(super) fn remove(host: &Shared, root: &Path, provider: Provider, id: &str) -> Result<()> {
    let account = {
        let h = host.lock().unwrap();
        let account = resolve(&h, provider, Some(id))?;
        ensure!(
            !account.is_default(),
            "The default account belongs to the provider CLI and cannot be removed"
        );
        ensure_idle(&h, &account, "removing it")?;
        // A live session keeps running under the removed identity until its process exits.
        for task in tasks_of(&h, &account)? {
            if let Some(session) = h.sessions.get(&task.id) {
                let _ = session.send(provider::Control::Close);
            }
        }
        account
    };
    // Sign out first so the credential store keeps no entry for a deleted directory.
    let _ = run_logout(&account);
    let mut h = host.lock().unwrap();
    h.store.remove_account(provider, id)?;
    h.accounts.remove(&(provider, id.to_owned()));
    if let Some(directory) = &account.home {
        remove_directory(root, directory)?;
    }
    Ok(())
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn cli_arguments(command: &'static str) -> impl Iterator<Item = &'static str> {
    command.split_whitespace().skip(1)
}

fn write_sign_in_script(root: &Path, account: &Account) -> Result<PathBuf> {
    let executable = provider::executable(account.provider)?;
    let mut script = String::from(
        "#!/bin/zsh\n# Generated by Kodem. Runs the provider's own sign-in for one account.\n",
    );
    for variable in account.cleared_variables() {
        script.push_str(&format!("unset {variable}\n"));
    }
    if let (Some(variable), Some(home)) = (account.provider.home_variable(), &account.home) {
        script.push_str(&format!(
            "export {variable}={}\n",
            quote(&home.to_string_lossy())
        ));
    }
    script.push_str(&quote(&executable.to_string_lossy()));
    for argument in cli_arguments(account.provider.login_command()) {
        script.push(' ');
        script.push_str(&quote(argument));
    }
    script.push_str(
        "\necho\necho 'Sign-in finished. You can close this window and return to Kodem.'\n",
    );
    fs::create_dir_all(accounts_root(root))?;
    fs::set_permissions(accounts_root(root), fs::Permissions::from_mode(0o700))?;
    let path = accounts_root(root).join(format!(
        "sign-in-{}-{}.command",
        format!("{:?}", account.provider).to_lowercase(),
        account.id
    ));
    fs::write(&path, script)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

/// Opens the provider CLI's own sign-in in Terminal and watches for its result.
pub(super) fn sign_in(host: &Shared, root: &Path, provider: Provider, id: &str) -> Result<()> {
    let (account, before) = {
        let h = host.lock().unwrap();
        let account = resolve(&h, provider, Some(id))?;
        ensure_idle(&h, &account, "signing in again")?;
        let status = h
            .accounts
            .get(&(provider, id.to_owned()))
            .cloned()
            .unwrap_or_default();
        ensure!(
            !status.signing_in,
            "A sign-in window is already open for this account"
        );
        (account, status)
    };
    if let Some(home) = &account.home {
        fs::create_dir_all(home)?;
        link_shared(provider, home)?;
    }
    let script = write_sign_in_script(root, &account)?;
    let opened = output(
        {
            let mut command = Command::new("/usr/bin/open");
            command.arg(&script);
            command
        },
        Duration::from_secs(15),
    )?;
    ensure!(
        opened.status.success(),
        "Could not open Terminal for sign-in. Run {} there instead.",
        provider.login_command()
    );
    {
        let mut h = host.lock().unwrap();
        let status = h.accounts.entry((provider, id.to_owned())).or_default();
        status.signing_in = true;
        status.error = None;
    }
    let host = host.clone();
    std::thread::spawn(move || {
        let key = (account.provider, account.id.clone());
        let deadline = Instant::now() + SIGN_IN_WAIT;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_secs(3));
            // A manual refresh, sign-out or removal ends the wait.
            if !host
                .lock()
                .unwrap()
                .accounts
                .get(&key)
                .is_some_and(|status| status.signing_in)
            {
                return;
            }
            let Ok(observed) = provider::account_status(&host, &account, false) else {
                continue;
            };
            let finished = observed.signed_in == Some(true)
                && (before.signed_in != Some(true)
                    || (observed.email.is_some() && observed.email != before.email));
            if finished {
                break;
            }
        }
        if let Some(status) = host.lock().unwrap().accounts.get_mut(&key) {
            status.signing_in = false;
        }
        let _ = refresh(&host, account.provider, &account.id);
    });
    Ok(())
}

fn run_logout(account: &Account) -> Result<()> {
    let mut command = Command::new(provider::executable(account.provider)?);
    command.args(cli_arguments(account.provider.logout_command()));
    account.apply(&mut command);
    let result = output(command, Duration::from_secs(30))?;
    ensure!(
        result.status.success(),
        "{} could not sign out: {}",
        account.provider.label(),
        String::from_utf8_lossy(&result.stderr)
            .trim()
            .chars()
            .take(400)
            .collect::<String>()
    );
    Ok(())
}

/// Removes an account's saved sign-in through the provider CLI.
pub(super) fn sign_out(host: &Shared, provider: Provider, id: &str) -> Result<()> {
    let account = {
        let h = host.lock().unwrap();
        let account = resolve(&h, provider, Some(id))?;
        ensure_idle(&h, &account, "signing out")?;
        for task in tasks_of(&h, &account)? {
            if let Some(session) = h.sessions.get(&task.id) {
                let _ = session.send(provider::Control::Close);
            }
        }
        account
    };
    run_logout(&account)?;
    {
        let mut h = host.lock().unwrap();
        let status = h.accounts.entry((provider, id.to_owned())).or_default();
        *status = AccountStatus {
            signed_in: Some(false),
            checked: now(),
            ..AccountStatus::default()
        };
    }
    refresh(host, provider, id)
}

fn read_status(host: &Shared, account: &Account, usage: bool) {
    let observed =
        provider::account_status(host, account, usage).unwrap_or_else(|error| AccountStatus {
            error: Some(format!("{error:#}")),
            ..AccountStatus::default()
        });
    let mut h = host.lock().unwrap();
    // An unreadable status must not erase the last identity the CLI reported.
    if observed.signed_in.is_none()
        && let Some(previous) = h.accounts.get(&(account.provider, account.id.clone()))
    {
        let mut kept = previous.clone();
        kept.error = observed.error;
        observe(&mut h, account, kept);
        return;
    }
    observe(&mut h, account, observed);
}

/// Reads one account's sign-in state and quotas again without starting a model turn.
pub(super) fn refresh(host: &Shared, provider: Provider, id: &str) -> Result<()> {
    let account = {
        let mut h = host.lock().unwrap();
        let account = resolve(&h, provider, Some(id))?;
        if let Some(status) = h.accounts.get_mut(&(provider, id.to_owned())) {
            status.signing_in = false;
        }
        account
    };
    read_status(host, &account, true);
    Ok(())
}

/// Reports that the provider refuses further use of an account until a reset, as last observed.
pub(super) fn limit_reached(host: &Host, provider: Provider, id: &str) -> bool {
    host.accounts
        .get(&(provider, id.to_owned()))
        .and_then(|status| status.usage.as_ref())
        .is_some_and(UsageLimits::limit_reached)
}

// Returns the accounts that a working or recently changed task uses, and the account that the
// next new task starts with.
fn in_use(host: &Host) -> Result<std::collections::BTreeSet<(Provider, String)>> {
    let defaults = host.store.settings()?.agent_defaults;
    let mut used = std::collections::BTreeSet::from([(
        defaults.provider,
        defaults.account.unwrap_or_else(|| DEFAULT_ACCOUNT.into()),
    )]);
    let recent = now().saturating_sub(RECENT_TASK_MS);
    for task in host.store.tasks()? {
        if !task.archived && (task.state.active() || task.updated >= recent) {
            used.insert((task.provider, task.account_id().to_owned()));
        }
    }
    Ok(used)
}

/// Refreshes accounts in the background cadence, reusing quotas a session just reported.
///
/// Accounts in use are read on every pass. An idle account is read once per `IDLE_CHECK_MS`.
pub(super) fn refresh_all(host: &Shared) -> Result<()> {
    let (states, used) = {
        let h = host.lock().unwrap();
        (states(&h)?, in_use(&h)?)
    };
    for state in states {
        if state.status.signing_in {
            continue;
        }
        let active = used.contains(&(state.account.provider, state.account.id.clone()));
        if !active && now().saturating_sub(state.status.checked) < IDLE_CHECK_MS {
            continue;
        }
        let fresh = state
            .status
            .usage
            .as_ref()
            .is_some_and(|usage| now().saturating_sub(usage.reported) < USAGE_FRESH_MS);
        read_status(host, &state.account, !fresh);
    }
    Ok(())
}
