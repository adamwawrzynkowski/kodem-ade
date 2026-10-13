//! Saves and restores the working tree of a task as commits under hidden references.
//!
//! A checkpoint holds every file of the repository that Git does not ignore, tracked or not.
//! Taking one changes neither the user's index nor a branch: the tree is built in a copy of the
//! index, and the commit is reachable only from `refs/kodem/checkpoints/<task>/<name>`. Ignored
//! files and other repositories that the checkout links to are not part of a checkpoint.
use super::*;

const NAMESPACE: &str = "refs/kodem/checkpoints";

// Git with a private index and an identity of its own, so that a checkpoint needs no user
// configuration and never moves the user's staging area.
fn git(root: &Path, index: Option<&Path>, args: &[&str]) -> Result<Vec<u8>> {
    let mut process = git_process(root)?;
    process
        .env("GIT_AUTHOR_NAME", "Kodem")
        .env("GIT_AUTHOR_EMAIL", "checkpoint@kodem.invalid")
        .env("GIT_COMMITTER_NAME", "Kodem")
        .env("GIT_COMMITTER_EMAIL", "checkpoint@kodem.invalid");
    if let Some(index) = index {
        process.env("GIT_INDEX_FILE", index);
    }
    let out = process.args(args).output()?;
    ensure!(
        out.status.success(),
        "Git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

fn line(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

// References are built from these names, so they must not carry path or revision syntax.
fn reference(task_id: &str, name: &str) -> Result<String> {
    for part in [task_id, name] {
        ensure!(
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "Invalid checkpoint name"
        );
    }
    Ok(format!("{NAMESPACE}/{task_id}/{name}"))
}

// Builds the tree of the working directory in the given private index and commits it on top
// of the current commit, without a reference.
fn commit_tree(root: &Path, index: &Path, task_id: &str, name: &str) -> Result<String> {
    let index = Some(index);
    git(root, index, &["add", "--all"])?;
    let tree = line(git(root, index, &["write-tree"])?);
    let message = format!("Kodem checkpoint {name} of task {task_id}");
    let mut arguments = vec!["commit-tree", tree.as_str(), "-m", message.as_str()];
    let parent = head(root);
    if let Some(parent) = &parent {
        arguments.extend(["-p", parent.as_str()]);
    }
    Ok(line(git(root, None, &arguments)?))
}

/// Saves the working tree of the repository at `cwd` and returns the reference that holds it.
///
/// A checkpoint of the same task and name is replaced.
pub fn capture(cwd: &Path, task_id: &str, name: &str) -> Result<String> {
    let reference = reference(task_id, name)?;
    let root = repository(cwd)?;
    let directory = PathBuf::from(line(git(
        &root,
        None,
        &["rev-parse", "--absolute-git-dir"],
    )?));
    let own = PathBuf::from(line(git(
        &root,
        None,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?));
    let index = directory.join(format!("kodem-checkpoint-{}.index", crate::new_id()));
    // A copy of the index keeps its record of unchanged files, so only changed files are read.
    if own.is_file() {
        fs::copy(&own, &index)?;
    }
    let commit = commit_tree(&root, &index, task_id, name);
    let _ = fs::remove_file(&index);
    git(&root, None, &["update-ref", &reference, &commit?])?;
    Ok(reference)
}

/// Makes the working tree equal to a checkpoint of the task and returns a new checkpoint of the
/// state it replaced.
///
/// Files are changed, created and removed; the branch, its commits and the index stay as they
/// are. A commit made after the checkpoint therefore remains, and its changes show as undone in
/// the working tree. The returned checkpoint lets the caller undo the restore.
pub fn restore(cwd: &Path, task_id: &str, name: &str) -> Result<String> {
    let target = reference(task_id, name)?;
    let root = repository(cwd)?;
    git(
        &root,
        None,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{target}^{{commit}}"),
        ],
    )
    .context("This checkpoint no longer exists")?;
    let backup = capture(&root, task_id, &format!("restore-{}", crate::now()))?;
    // Files that exist now and did not exist in the checkpoint. Git removes the tracked ones
    // below; an untracked one is known only from the checkpoint taken a moment ago.
    let added = git(
        &root,
        None,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--diff-filter=A",
            &target,
            &backup,
        ],
    )?;
    for path in added
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let _ = fs::remove_file(root.join(OsStr::from_bytes(path)));
    }
    git(
        &root,
        None,
        &[
            "restore",
            &format!("--source={target}"),
            "--worktree",
            "--",
            ".",
        ],
    )?;
    Ok(backup)
}

/// Writes the files of a checkpoint into another working tree of the same repository, such as
/// the new worktree of a fork. Nothing is saved first, so the tree should hold no work.
pub fn apply(cwd: &Path, task_id: &str, name: &str) -> Result<()> {
    let source = reference(task_id, name)?;
    let root = repository(cwd)?;
    git(
        &root,
        None,
        &[
            "restore",
            &format!("--source={source}"),
            "--worktree",
            "--",
            ".",
        ],
    )?;
    Ok(())
}

/// Returns the reference of a checkpoint, for commands that name its commit.
pub fn reference_of(task_id: &str, name: &str) -> Result<String> {
    reference(task_id, name)
}

/// Deletes every checkpoint of a task. The repository removes their content when it next
/// collects unreachable objects.
pub fn discard(cwd: &Path, task_id: &str) -> Result<()> {
    let prefix = reference(task_id, "x")?;
    let prefix = prefix.trim_end_matches('x');
    let root = repository(cwd)?;
    let references = git(
        &root,
        None,
        &["for-each-ref", "--format=%(refname)", prefix],
    )?;
    for reference in String::from_utf8_lossy(&references).lines() {
        git(&root, None, &["update-ref", "-d", reference])?;
    }
    Ok(())
}
