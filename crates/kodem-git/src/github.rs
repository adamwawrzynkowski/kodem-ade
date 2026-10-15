use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

/// One pull request row.
#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    #[allow(dead_code)]
    pub state: String,
    pub draft: bool,
    pub html_url: String,
    pub user: PrUser,
    pub head: PrRef,
    pub base: PrRef,
    pub updated_at: String,
}

/// The author of a pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct PrUser {
    pub login: String,
}

/// A branch that a pull request merges from or into.
#[derive(Debug, Clone, Deserialize)]
pub struct PrRef {
    #[serde(rename = "ref")]
    pub name: String,
}

/// The signed-in user behind a token.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountUser {
    pub login: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AccountEmail {
    email: String,
    primary: bool,
}

/// One repository that an account can clone.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteRepository {
    pub full_name: String,
    pub clone_url: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub stargazers_count: u64,
}

/// The REST API root for a GitHub or GitHub Enterprise host.
pub fn api_base(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.is_empty() || host == "github.com" {
        "https://api.github.com".to_string()
    } else {
        format!("https://{host}/api/v3")
    }
}

/// The page where the user creates a token with the scopes Kodem needs.
pub fn new_token_url(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    let host = if host.is_empty() { "github.com" } else { host };
    format!("https://{host}/settings/tokens/new?scopes=repo,read:user,user:email&description=Kodem")
}

/// Runs a GET request with the system curl tool and returns the body.
/// The token goes to curl on stdin as a header file, so it never shows
/// in the process list.
fn api_get(url: &str, token: &str) -> Result<String> {
    use std::io::Write as _;
    use std::process::Stdio;

    let mut child = crate::process::command("curl")
        .args([
            "-sS",
            "--fail-with-body",
            "--max-time",
            "20",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: Kodem",
            "-H",
            "@-",
        ])
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run curl")?;
    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "Authorization: Bearer {token}").context("failed to send the token")?;
    }
    let output = child.wait_with_output().context("failed to run curl")?;
    if !output.status.success() {
        let body = String::from_utf8_lossy(&output.stdout);
        if body.contains("Bad credentials") {
            bail!("The token is not valid. Check that you copied all of it.");
        }
        if body.trim().is_empty() {
            bail!("Kodem could not reach the server. Check the network and the host name.");
        }
        bail!("The server refused the request. Check the token scopes.");
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Finds the user that owns `token`, with the primary email when the
/// profile hides it. Blocking; run on a background thread.
pub fn account_user(host: &str, token: &str) -> Result<AccountUser> {
    let base = api_base(host);
    let body = api_get(&format!("{base}/user"), token)?;
    let mut user: AccountUser = serde_json::from_str(&body).context("unexpected response")?;
    if user.email.as_deref().is_none_or(str::is_empty)
        && let Ok(body) = api_get(&format!("{base}/user/emails"), token)
        && let Ok(emails) = serde_json::from_str::<Vec<AccountEmail>>(&body)
    {
        user.email = emails
            .into_iter()
            .find(|email| email.primary)
            .map(|email| email.email);
    }
    Ok(user)
}

/// The repositories the account owns, works on, or can see through its
/// organizations, most recently updated first. Blocking; run on a
/// background thread.
pub fn account_repositories(host: &str, token: &str) -> Result<Vec<RemoteRepository>> {
    let base = api_base(host);
    let mut repositories = Vec::new();
    // Three pages cover most accounts without a long wait.
    for page in 1..=3 {
        let body = api_get(
            &format!(
                "{base}/user/repos?per_page=100&page={page}&sort=updated\
                 &affiliation=owner,collaborator,organization_member"
            ),
            token,
        )?;
        let batch: Vec<RemoteRepository> =
            serde_json::from_str(&body).context("unexpected response")?;
        let done = batch.len() < 100;
        repositories.extend(batch);
        if done {
            break;
        }
    }
    Ok(repositories)
}

/// The token that the GitHub CLI saved for `host`, when the CLI is
/// installed and signed in. Blocking; run on a background thread.
pub fn github_cli_token(host: &str) -> Option<String> {
    let host = if host.trim().is_empty() {
        "github.com"
    } else {
        host.trim()
    };
    let mut command = crate::process::command("gh");
    command.args(["auth", "token", "--hostname", host]);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Extract "owner/repo" from a GitHub remote URL.
pub fn github_slug(remote_url: &str) -> Option<String> {
    let url = remote_url.trim().trim_end_matches(".git");
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return Some(rest.to_string());
    }
    if let Some(pos) = url.find("github.com/") {
        return Some(url[pos + "github.com/".len()..].to_string());
    }
    None
}

/// Load open pull requests with the system curl tool. Blocking; run on a
/// background thread.
pub fn list_pull_requests(slug: &str, token: Option<&str>) -> Result<Vec<PullRequest>> {
    let url = format!("https://api.github.com/repos/{slug}/pulls?state=open&per_page=50");
    let mut cmd = crate::process::command("curl");
    cmd.args([
        "-sf",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "User-Agent: Kodem",
    ]);
    if let Some(token) = token {
        cmd.arg("-H").arg(format!("Authorization: Bearer {token}"));
    }
    cmd.arg(&url);
    let output = cmd.output().context("failed to run curl")?;
    if !output.status.success() {
        bail!("GitHub request failed. Check the network and the account token.");
    }
    let body = String::from_utf8_lossy(&output.stdout);
    let prs: Vec<PullRequest> = serde_json::from_str(&body).context("unexpected response")?;
    Ok(prs)
}

/// Loads the diff of a pull request with the system curl tool. Fails when the request fails or the
/// diff is empty. Blocking; run on a background thread.
pub fn pull_request_diff(slug: &str, number: u64, token: Option<&str>) -> Result<String> {
    let url = format!("https://api.github.com/repos/{slug}/pulls/{number}");
    let mut cmd = crate::process::command("curl");
    cmd.args([
        "-sf",
        "-H",
        "Accept: application/vnd.github.v3.diff",
        "-H",
        "User-Agent: Kodem",
    ]);
    if let Some(token) = token {
        cmd.arg("-H").arg(format!("Authorization: Bearer {token}"));
    }
    cmd.arg(&url);
    let output = cmd.output().context("failed to run curl")?;
    if !output.status.success() {
        bail!("Kodem could not load the pull request diff.");
    }
    let diff = String::from_utf8_lossy(&output.stdout).into_owned();
    if diff.trim().is_empty() {
        bail!("The pull request has no diff.");
    }
    Ok(diff)
}
