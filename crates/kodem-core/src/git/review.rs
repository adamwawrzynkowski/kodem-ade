//! Review targets: a project's open pull requests and branches, and a detached worktree at a
//! target's head. Nothing here pushes, comments, or moves a branch or the project's checkout.
use super::*;

/// Most pull requests listed for one project.
const PULL_REQUEST_LIMIT: usize = 60;
/// Most branches listed for one project, newest commit first.
const BRANCH_LIMIT: usize = 200;

/// A detached checkout of a review target with what the target resolved to.
pub struct PreparedReview {
    pub cwd: PathBuf,
    /// Reviewed commit, which the worktree has checked out.
    pub head: String,
    /// Merge base of `head` and the tip of `base`.
    pub merge_base: String,
    /// Name of the branch that the target is compared with.
    pub base: String,
}

// Runs GitHub CLI in a checkout. It never prompts, and its absence reads as a setup hint.
fn github(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = platform::hide_console(&mut Command::new("gh"))
        .current_dir(cwd)
        .env("PATH", search_path()?)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("GitHub CLI is not installed. Install `gh` and run `gh auth login`")?;
    ensure!(
        out.status.success(),
        "GitHub CLI: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

// Parses the UTC timestamps GitHub reports, such as `2026-10-11T12:34:56Z`, to milliseconds.
fn timestamp(text: &str) -> Option<u64> {
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days since 1970-01-01 in the proleptic Gregorian calendar, with March as the first month.
    let year = if month <= 2 { year - 1 } else { year };
    let (era, year_of_era) = (year.div_euclid(400), year.rem_euclid(400));
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146097 + day_of_era - 719468;
    u64::try_from((days * 86400 + hour * 3600 + minute * 60 + second) * 1000).ok()
}

fn pull_requests(project: &Path) -> Result<Vec<PullRequestSummary>> {
    let limit = PULL_REQUEST_LIMIT.to_string();
    let bytes = github(
        project,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            &limit,
            "--json",
            "number,title,headRefName,baseRefName,author,isDraft,updatedAt,additions,deletions,url,isCrossRepository,headRepositoryOwner",
        ],
    )?;
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
        .context("GitHub CLI returned an unreadable pull request list")?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let branch = row["headRefName"].as_str()?;
            let owner = row["headRepositoryOwner"]["login"]
                .as_str()
                .filter(|_| row["isCrossRepository"] == true);
            Some(PullRequestSummary {
                number: row["number"].as_u64()?,
                title: row["title"].as_str().unwrap_or_default().to_owned(),
                head: match owner {
                    Some(owner) => format!("{owner}:{branch}"),
                    None => branch.to_owned(),
                },
                base: row["baseRefName"].as_str()?.to_owned(),
                author: row["author"]["login"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                draft: row["isDraft"] == true,
                updated: row["updatedAt"].as_str().and_then(timestamp).unwrap_or(0),
                additions: row["additions"].as_u64().unwrap_or(0),
                deletions: row["deletions"].as_u64().unwrap_or(0),
                url: row["url"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect())
}

// Lists local and remote-tracking branches, newest commit first. The default branch is what the
// others are compared with, so it is not listed. A remote-tracking branch is not listed when a
// local branch of the same name is at the same commit.
fn branches(project: &Path, default: Option<&str>) -> Result<Vec<BranchSummary>> {
    let count = format!("--count={BRANCH_LIMIT}");
    let bytes = query(
        project,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            &count,
            "--format=%(refname)%00%(objectname:short)%00%(committerdate:unix)%00%(authorname)%00%(contents:subject)%00%(HEAD)%00%(symref)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let text = String::from_utf8_lossy(&bytes);
    let mut listed = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\0').collect();
        let &[reference, commit, date, author, subject, current, symbolic] = fields.as_slice()
        else {
            continue;
        };
        // A remote's `HEAD` is a pointer to one of its branches, not a branch.
        if !symbolic.is_empty() {
            continue;
        }
        let (name, remote) = match reference.strip_prefix("refs/heads/") {
            Some(name) => (name, false),
            None => match reference.strip_prefix("refs/remotes/") {
                Some(name) => (name, true),
                None => continue,
            },
        };
        // The name of a remote-tracking branch without its remote.
        let plain = if remote {
            name.split_once('/').map_or(name, |(_, branch)| branch)
        } else {
            name
        };
        if default == Some(plain) {
            continue;
        }
        listed.push(BranchSummary {
            name: name.to_owned(),
            reference: reference.to_owned(),
            remote,
            current: current == "*",
            commit: commit.to_owned(),
            subject: subject.to_owned(),
            author: author.to_owned(),
            updated: date.parse::<u64>().map_or(0, |seconds| seconds * 1000),
        });
    }
    let local: Vec<(String, String)> = listed
        .iter()
        .filter(|branch| !branch.remote)
        .map(|branch| (branch.name.clone(), branch.commit.clone()))
        .collect();
    listed.retain(|branch| {
        !branch.remote
            || !local.iter().any(|(name, commit)| {
                branch.name.split_once('/').map(|(_, plain)| plain) == Some(name.as_str())
                    && &branch.commit == commit
            })
    });
    Ok(listed)
}

/// Lists what can be reviewed in a project: its open pull requests and its branches.
///
/// A failure of GitHub CLI is reported with the result, so the branches still list.
pub fn review_targets(project_id: &str, project: &Path) -> Result<ReviewTargets> {
    let default_branch = default_branch(project).map(|(name, _)| name);
    let (pull_requests, pull_request_error) = match pull_requests(project) {
        Ok(list) => (list, None),
        Err(error) => (Vec::new(), Some(format!("{error:#}"))),
    };
    Ok(ReviewTargets {
        project_id: project_id.to_owned(),
        branches: branches(project, default_branch.as_deref())?,
        default_branch,
        pull_requests,
        pull_request_error,
        dirty: query(
            project,
            &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
        )
        .is_ok_and(|status| !status.is_empty()),
    })
}

// Reports a name that is safe to place in a refspec: no option, range or pattern syntax.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.contains("..")
        && !name.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
}

// Returns the remote of the checkout that points to the repository of a pull request address,
// such as `https://github.com/owner/repository/pull/42`. Fetching through that remote uses the
// sign-in the user already has for it.
fn pull_request_remote(project: &Path, url: &str) -> Result<String> {
    let mut parts = url.split("://").nth(1).unwrap_or(url).split('/').skip(1);
    let (Some(owner), Some(repository)) = (parts.next(), parts.next()) else {
        bail!("The pull request has no repository address");
    };
    let wanted = format!("{owner}/{repository}").to_lowercase();
    let remotes = String::from_utf8_lossy(&query(project, &["remote", "-v"])?).into_owned();
    remotes
        .lines()
        .filter(|line| line.ends_with("(fetch)"))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?))
        })
        .find(|(_, address)| {
            let address = address.to_lowercase();
            let address = address.trim_end_matches('/').trim_end_matches(".git");
            address.ends_with(&format!("/{wanted}")) || address.ends_with(&format!(":{wanted}"))
        })
        .map(|(name, _)| name.to_owned())
        .filter(|name| plain_name(name))
        .with_context(|| format!("No Git remote of this checkout points to {wanted}"))
}

// Returns the commit to review, the tip of the branch it is compared with, and that branch's
// name. A pull request is fetched into references that belong to this review alone.
fn resolve_target(
    project: &Path,
    id: &str,
    target: &ReviewTarget,
) -> Result<(String, String, String)> {
    match target {
        ReviewTarget::PullRequest {
            number, base, url, ..
        } => {
            ensure!(
                plain_name(base),
                "The pull request names an invalid base branch"
            );
            let remote = pull_request_remote(project, url)?;
            let head_reference = format!("refs/kodem/reviews/{id}/head");
            let base_reference = format!("refs/kodem/reviews/{id}/base");
            query(
                project,
                &[
                    "fetch",
                    "--no-tags",
                    "--no-write-fetch-head",
                    "--no-recurse-submodules",
                    &remote,
                    &format!("+refs/pull/{number}/head:{head_reference}"),
                    &format!("+refs/heads/{base}:{base_reference}"),
                ],
            )
            .with_context(|| format!("Could not fetch pull request #{number} from {remote}"))?;
            Ok((
                resolve_commit(project, &head_reference)
                    .context("The pull request has no head commit")?,
                resolve_commit(project, &base_reference)
                    .context("The pull request has no base commit")?,
                base.clone(),
            ))
        }
        ReviewTarget::Branch { reference, .. } => {
            ensure!(
                reference.starts_with("refs/heads/") || reference.starts_with("refs/remotes/"),
                "Choose a branch of this repository"
            );
            let head = resolve_commit(project, reference)
                .context("This branch no longer exists. Refresh the list.")?;
            let (base, remote) = default_branch(project)
                .context("This repository has no default branch to compare with")?;
            let tip = resolve_commit(project, &format!("refs/heads/{base}"))
                .or_else(|| remote.and_then(|reference| resolve_commit(project, &reference)))
                .context("The default branch has no commit to compare with")?;
            Ok((head, tip, base))
        }
    }
}

/// Checks out the head of a review target in a detached worktree under `root`.
///
/// A pull request is fetched from the remote of its repository; a branch is read as the local
/// repository knows it. The project's own checkout, branches and index are not changed.
pub fn prepare_review(
    project: &Path,
    root: &Path,
    id: &str,
    target: &ReviewTarget,
) -> Result<PreparedReview> {
    ensure!(
        head(project).is_some(),
        "Create an initial commit before starting a review"
    );
    let label = target.label();
    let prepared = resolve_target(project, id, target).and_then(|(head, tip, base)| {
        let merge_base = query(project, &["merge-base", &head, &tip])
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .with_context(|| format!("{label} and {base} share no history"))?;
        ensure!(
            merge_base != head,
            "Nothing to review: {label} has no commits that {base} lacks"
        );
        fs::create_dir_all(root)?;
        let cwd = root.join(id);
        command(
            project,
            &[
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--detach"),
                cwd.as_os_str(),
                OsStr::new(&head),
            ],
        )?;
        Ok(PreparedReview {
            cwd,
            head,
            merge_base,
            base,
        })
    });
    // The worktree keeps the reviewed commits reachable, so the fetched references can go.
    if matches!(target, ReviewTarget::PullRequest { .. }) {
        for name in ["head", "base"] {
            let _ = query(
                project,
                &[
                    "update-ref",
                    "-d",
                    &format!("refs/kodem/reviews/{id}/{name}"),
                ],
            );
        }
    }
    prepared
}

/// Removes the detached worktree of a review. It holds no branch, so no commit is lost.
pub fn discard_review(project: Option<&Path>, cwd: &Path) -> Result<()> {
    if let Some(project) = project
        && command(
            project,
            &[
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                cwd.as_os_str(),
            ],
        )
        .is_ok()
    {
        return Ok(());
    }
    if cwd.exists() {
        fs::remove_dir_all(cwd)?;
    }
    Ok(())
}
