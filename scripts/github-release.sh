#!/bin/bash
# Shared release metadata and publication steps.
#
# Every platform releases together in GitHub Actions: the Release packages workflow
# (.github/workflows/release.yml) chooses the version, builds macOS, Windows (x64 and Arm64)
# and Linux, and publishes one release with all of them. Start it with Run workflow on GitHub,
# or with scripts/release.sh (the Build Release commands), which follows its progress.
#
# The workflow's builds run with KODEM_RELEASE_VERSION set: the version is fixed and they only
# package. Run directly with a release channel, a platform's build script publishes that
# platform alone, or adds it to the release of the same commit.
release_repository=adamwawrzynkowski/kodem-ade
release_workflow=release.yml
# The release highlights that the What's new board draws (see its header for the format).
release_highlights_file=changelog/highlights.toml
# The subject prefix of commits that change only the highlights. The change list leaves them out.
release_changelog_prefix='Changelog:'
# The subject prefix of commits that release notes skip: the highlights gate does not ask for
# them, and the change list leaves them out.
release_hidden_prefix='hidden:'

# Fails unless GitHub CLI's account may create release tags. The Release tags ruleset reserves
# them for the owner, collaborators and the release workflow, whose job permissions decide.
kodem_release_check_account() {
    [[ "${GITHUB_ACTIONS:-}" != true ]] || return 0
    [[ "$(gh api "repos/$release_repository" --jq .permissions.push 2>/dev/null)" == true ]] || { echo "GitHub CLI is signed in as $(gh api user --jq .login 2>/dev/null || echo 'an unknown account'), which cannot create release tags. Run gh auth switch to the owner or a collaborator account." >&2; return 1; }
}

# Resolves a monotonically increasing channel version before compiling, or the version of the
# release that another platform already published for this commit.
kodem_release_prepare() {
    local channel=$1 base tags previous candidate releases pattern tag target draft
    base=$(cargo pkgid -p kodem-desktop)
    base=${base##*#}
    base=${base##*@}
    release_version=$base
    # The commit subjects since the previous release of the channel and the ids of the highlights
    # that it did not have, embedded for the app's What's new dialog. Debug builds have no commit
    # list and preview the highlights that the newest Nightly in this clone lacks.
    release_previous=""
    release_notes=""
    release_highlights=""
    if [[ "$channel" == debug ]]; then
        kodem_release_local_highlights
        return 0
    fi
    release_existing=""
    if [[ -n "${KODEM_RELEASE_VERSION:-}" ]]; then
        # Part of a release that another build publishes: the version is given, and this build
        # neither reads nor writes GitHub releases.
        [[ "$KODEM_RELEASE_VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || { echo 'KODEM_RELEASE_VERSION must be X.X.X without leading zeros.' >&2; return 1; }
        [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Release builds require a clean committed checkout.' >&2; return 1; }
        release_commit=$(git rev-parse HEAD)
        [[ -z "${KODEM_RELEASE_COMMIT:-}" || "$KODEM_RELEASE_COMMIT" == "$release_commit" ]] || { echo "This checkout is not the release commit $KODEM_RELEASE_COMMIT." >&2; return 1; }
        release_version=$KODEM_RELEASE_VERSION
        release_previous=${KODEM_RELEASE_PREVIOUS:-}
        release_notes=${KODEM_RELEASE_NOTES:-}
        release_highlights=${KODEM_RELEASE_HIGHLIGHTS:-}
        release_tag="v$release_version"
        [[ "$channel" != nightly ]] || release_tag="nightly/$release_tag"
        echo "Release package: $release_tag at $release_commit (published by the build that started this one)"
        return 0
    fi
    command -v gh >/dev/null || { echo 'Install GitHub CLI and run gh auth login before publishing.' >&2; return 1; }
    # In the release workflow, GH_TOKEN carries the job's token, which has no user account.
    if [[ "${GITHUB_ACTIONS:-}" != true ]]; then
        gh auth status >/dev/null 2>&1 || { echo 'Run gh auth login with the owner or a collaborator account.' >&2; return 1; }
    fi
    kodem_release_check_account
    [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Release builds require a clean committed checkout. No changes are committed or pushed by this script.' >&2; return 1; }
    release_commit=$(git rev-parse HEAD)
    [[ "$(gh api "repos/$release_repository/commits/$release_commit" --jq .sha)" == "$release_commit" ]] || { echo 'The build commit must already exist on GitHub.' >&2; return 1; }
    releases=$(gh api --paginate "repos/$release_repository/releases?per_page=100" --jq '.[] | [.tag_name, .target_commitish, (.draft | tostring)] | @tsv')
    tags=$(printf '%s\n' "$releases" | cut -f 1)
    pattern='^v[0-9]+\.[0-9]+\.[0-9]+$'
    [[ "$channel" != nightly ]] || pattern='^nightly/v[0-9]+\.[0-9]+\.[0-9]+$'
    while IFS=$'\t' read -r tag target draft; do
        [[ "$target" == "$release_commit" && "$tag" =~ $pattern ]] || continue
        [[ "$draft" == false ]] || { echo "Release $tag for this commit is still a draft. Publish or delete it, then retry." >&2; return 1; }
        release_existing=$tag
    done <<< "$releases"
    if [[ -n "$release_existing" ]]; then
        release_tag=$release_existing
        release_version=${release_tag#nightly/}
        release_version=${release_version#v}
        [[ -z "${KODEM_VERSION:-}" || "$KODEM_VERSION" == "$release_version" ]] || { echo "This commit is already released as $release_tag. Unset KODEM_VERSION to add this platform to it." >&2; return 1; }
        echo "Release: $release_repository $release_tag at $release_commit (adding this platform)"
        kodem_release_changes "$channel" "$tags"
        kodem_release_highlights
        return 0
    fi
    if [[ "$channel" == nightly ]]; then
        previous=$(printf '%s\n' "$tags" | sed -nE 's@^nightly/v([0-9]+\.[0-9]+\.[0-9]+)$@\1@p' | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
    else
        previous=$(printf '%s\n' "$tags" | sed -nE 's/^v([0-9]+\.[0-9]+\.[0-9]+)$/\1/p' | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
    fi
    if [[ -n "$previous" ]]; then
        candidate="${previous%.*}.$((${previous##*.} + 1))"
        release_version=$(printf '%s\n' "$base" "$candidate" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
    fi
    release_version=${KODEM_VERSION:-$release_version}
    [[ "$release_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || { echo 'KODEM_VERSION must be X.X.X without leading zeros.' >&2; return 1; }
    if [[ -n "$previous" ]]; then
        [[ "$release_version" != "$previous" && "$(printf '%s\n' "$release_version" "$previous" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)" == "$release_version" ]] || { echo "Version must be newer than $previous." >&2; return 1; }
    fi
    release_tag="v$release_version"
    [[ "$channel" != nightly ]] || release_tag="nightly/$release_tag"
    echo "Release: $release_repository $release_tag at $release_commit"
    kodem_release_changes "$channel" "$tags"
    kodem_release_check_changelog "$(kodem_release_tag "$channel" "$release_previous")" "$release_commit"
    kodem_release_highlights
}

# Prints the tag of a channel's version, or nothing for no version.
kodem_release_tag() {
    local channel=$1 version=$2
    [[ -n "$version" ]] || return 0
    if [[ "$channel" == nightly ]]; then echo "nightly/v$version"; else echo "v$version"; fi
}

# Prints the tag of the newest published release of a channel on GitHub, or nothing before its
# first one.
kodem_release_newest_tag() {
    local channel=$1 pattern='^v[0-9]+\.[0-9]+\.[0-9]+$' tags
    [[ "$channel" != nightly ]] || pattern='^nightly/v[0-9]+\.[0-9]+\.[0-9]+$'
    tags=$(gh api --paginate "repos/$release_repository/releases?per_page=100" --jq '.[] | select(.draft | not) | .tag_name') || return 1
    printf '%s\n' "$tags" | { grep -E "$pattern" || true; } | sed -E 's@^(nightly/)?v@@' | sort -t. -k1,1n -k2,2n -k3,3n | tail -1 | while read -r version; do kodem_release_tag "$channel" "$version"; done
}

# Prints the ids of the entries of a highlights file read from standard input. build.rs makes
# every entry write its id on a line of its own in exactly this form.
kodem_highlight_ids() {
    sed -nE 's/^id = "([a-z0-9-]+)"$/\1/p'
}

# Sets release_highlights to the ids of the highlights that the release before this one, the
# release_previous of the channel, did not have. Without an earlier release every entry is new.
# Missing highlights never stop a release: when GitHub cannot tell what the previous release
# had, this release shows none rather than old ones again.
kodem_release_highlights() {
    local previous_tag shipped current
    current=$(kodem_highlight_ids < "$release_highlights_file")
    previous_tag=$(kodem_release_tag "$channel" "$release_previous")
    if [[ -z "$previous_tag" ]]; then
        release_highlights=$current
    elif shipped=$(gh api -H 'Accept: application/vnd.github.raw+json' "repos/$release_repository/contents/$release_highlights_file?ref=$previous_tag" 2>/dev/null); then
        release_highlights=$(comm -23 <(printf '%s\n' "$current" | sort) <(printf '%s\n' "$shipped" | kodem_highlight_ids | sort))
    elif [[ "$(gh api "repos/$release_repository/commits?sha=$previous_tag&path=$release_highlights_file&per_page=1" --jq length 2>/dev/null)" == 0 ]]; then
        # The previous release came before the highlights file.
        release_highlights=$current
    else
        echo "Could not read the highlights of $previous_tag. This release shows no highlights." >&2
        release_highlights=""
    fi
    echo "Highlights new since ${previous_tag:-the first release}: $(printf '%s' "$release_highlights" | grep -c . || true)"
}

# Sets release_highlights for a Debug build to the ids that the newest Nightly tag in this clone
# lacks, so the build previews the board of the next Nightly. Every entry without such a tag.
kodem_release_local_highlights() {
    local tag
    release_highlights=$(kodem_highlight_ids < "$release_highlights_file")
    tag=$(git tag -l 'nightly/v*' --sort=-v:refname 2>/dev/null | head -1 || true)
    [[ -n "$tag" ]] || return 0
    release_highlights=$(comm -23 <(printf '%s\n' "$release_highlights" | sort) <(git show "$tag:$release_highlights_file" 2>/dev/null | kodem_highlight_ids | sort))
}

# Fails unless the highlights file reviewed every commit after a channel's previous release tag
# up to a commit: each commit after the file's `reviewed` commit must be a Changelog: commit
# that changes only the file, or a hidden: commit. Then prints the commits it lacks and a prompt
# for a coding model that writes the highlights, and copies the prompt where a clipboard command
# exists. Before the first release of a channel there is nothing to review.
kodem_release_check_changelog() {
    local previous_tag=$1 commit=$2 reviewed range after status sha subject files pending="" prompt
    [[ -n "$previous_tag" ]] || return 0
    reviewed=$(sed -nE 's/^reviewed = "([0-9a-f]{40})"$/\1/p' "$release_highlights_file")
    range=$(gh api "repos/$release_repository/compare/$previous_tag...$commit?per_page=250" --jq '.commits[] | select((.parents | length) < 2) | [.sha, (.commit.message | split("\n")[0])] | @tsv') \
        || { echo "Could not read the commits since $previous_tag, so the release highlights cannot be checked. Retry when GitHub answers." >&2; return 1; }
    if [[ -n "$reviewed" ]]; then
        after=$(gh api "repos/$release_repository/compare/$reviewed...$commit?per_page=250" --jq '.status, .commits[].sha') \
            || { echo "The reviewed commit $reviewed of $release_highlights_file is not on GitHub." >&2; return 1; }
        status=$(head -1 <<< "$after")
        [[ "$status" == ahead || "$status" == identical ]] || { echo "The reviewed commit $reviewed of $release_highlights_file is not an ancestor of $commit." >&2; return 1; }
    else
        after=$(cut -f 1 <<< "$range")
    fi
    while IFS=$'\t' read -r sha subject; do
        [[ -n "$sha" ]] && grep -Fxq "$sha" <<< "$after" || continue
        if [[ "$subject" == "$release_changelog_prefix"* ]]; then
            files=$(gh api "repos/$release_repository/commits/$sha" --jq '[.files[].filename] | join(" ")') \
                || { echo "Could not read the files of $sha." >&2; return 1; }
            [[ "$files" != "$release_highlights_file" ]] || continue
            echo "${sha:0:12} \"$subject\" changes more than $release_highlights_file. A $release_changelog_prefix commit may change only that file; reword it or split it." >&2
            return 1
        fi
        [[ "$subject" != "$release_hidden_prefix"* ]] || continue
        pending+="${sha:0:12} $subject"$'\n'
    done <<< "$range"
    [[ -n "$pending" ]] || { echo "Release highlights reviewed every change since $previous_tag."; return 0; }
    # read, not a command substitution: bash 3.2 misparses apostrophes in a heredoc inside $( ).
    IFS= read -r -d '' prompt <<PROMPT || true
Update the release highlights of Kodem in $release_highlights_file. Kodem's What's new dialog draws them as a board of cards, and every release shows the entries that its channel's previous release did not have.

These commits since $previous_tag are not reviewed yet, oldest first:
$pending
1. Read the header of $release_highlights_file. It defines the format, the visual kinds and their limits, and it is the only reference you need for the file.
2. Read every commit above with \`git show <commit>\`, and the entries of docs/implementation/progress.md that the commits record, to learn what changed for the user.
3. For each change that a user can notice, add a [[highlight]] above the existing highlights; group related commits into one card. Write them as release notes for the people who use Kodem, following the writing rules at the end of the header: a short human title and one or two plain sentences about what the user can do now or what got better, not the commit subject, no names from the code and no rendering or build terms, and never as a move from one version to another. Leave out refactors, internal and build changes, and fixes nobody could notice. It is fine to add no highlight when nothing is visible to users.
4. Give every card the visual kind that shows its change best, using the hints in the header, with true data only: measured figures from the commits or the progress notes, real names, real labels, real shortcuts and real states. Never invent numbers. Vary the design: a highlight's visual kind must differ from the kinds of the three highlights above it, which makes the existing highlights under the new ones count too, so choose the kind that fits each change best and order the new cards so that they alternate. Prefer widths that add up to 6, 10, 14… columns, so the board has no gaps.
5. When the changes are worth a sentence on the version card, add one [[headline]] above the existing headlines.
6. Set reviewed = "$commit", the newest commit that you reviewed.
7. Run \`cargo build -p kodem-desktop --locked\` (if cargo is not found, first run \`export PATH="\$HOME/.cargo/bin:\$PATH"\`). build.rs checks the file and reports each mistake; fix them all.
8. Commit only $release_highlights_file, with a subject that starts with "$release_changelog_prefix", for example "$release_changelog_prefix Highlights for the next release". Do not change other files, and do not push.
PROMPT
    {
        echo
        echo "The release highlights are not done: $release_highlights_file has not reviewed these commits since $previous_tag:"
        printf '%s' "$pending" | sed 's/^/  /'
        echo
        echo "Nothing was started. Send the prompt below to a coding model in this repository, push its $release_changelog_prefix commit, and release again."
        echo
        echo "----- prompt -----"
        printf '%s' "$prompt"
        echo "----- end of prompt -----"
    } >&2
    if [[ "${GITHUB_ACTIONS:-}" != true ]]; then
        if command -v pbcopy >/dev/null; then
            printf '%s' "$prompt" | pbcopy && echo "The prompt is on the clipboard." >&2
        elif command -v clip.exe >/dev/null; then
            printf '%s' "$prompt" | clip.exe && echo "The prompt is on the clipboard." >&2
        fi
    fi
    return 1
}

# Sets release_previous to the newest release of the channel before release_version, and
# release_notes to the subjects of the commits since it, newest first, without merges, without
# the Changelog: commits that only change the highlights and without the hidden: commits. GitHub
# compares up to 250 commits; the notes keep 100. Missing notes never stop a release.
kodem_release_changes() {
    local channel=$1 tags=$2 previous_tag base
    if [[ "$channel" == nightly ]]; then
        release_previous=$(printf '%s\n' "$tags" | sed -nE 's@^nightly/v([0-9]+\.[0-9]+\.[0-9]+)$@\1@p')
    else
        release_previous=$(printf '%s\n' "$tags" | sed -nE 's/^v([0-9]+\.[0-9]+\.[0-9]+)$/\1/p')
    fi
    release_previous=$(printf '%s\n' "$release_previous" "$release_version" | sed '/^$/d' | sort -u -t. -k1,1n -k2,2n -k3,3n | awk -v current="$release_version" '$0 == current { print previous; exit } { previous = $0 }')
    [[ -n "$release_previous" ]] || { echo "No earlier $channel release, so this release has no change list."; return 0; }
    previous_tag="v$release_previous"
    [[ "$channel" != nightly ]] || previous_tag="nightly/$previous_tag"
    if ! base=$(gh api "repos/$release_repository/git/ref/tags/$previous_tag" --jq .object.sha) \
        || ! release_notes=$(gh api "repos/$release_repository/compare/$base...$release_commit?per_page=250" --jq "[.commits[] | select((.parents | length) < 2) | .commit.message | split(\"\\n\")[0] | select(startswith(\"$release_changelog_prefix\") or startswith(\"$release_hidden_prefix\") | not)] | reverse | .[:100] | .[]"); then
        echo "Could not read the changes since $previous_tag. This release shows no change list." >&2
        release_notes=""
        return 0
    fi
    echo "Changes since $previous_tag: $(printf '%s' "$release_notes" | grep -c . || true) commits"
}

# Tells whether this build publishes its packages itself, as opposed to packaging them for the
# release workflow.
kodem_release_publishes() {
    [[ "$1" != debug && -z "${KODEM_RELEASE_VERSION:-}" ]]
}

# Starts the Release packages workflow for a commit and sets release_run to its run. The
# platforms come from KODEM_RELEASE_PLATFORMS (default: all) and the version from KODEM_VERSION
# when it is set.
kodem_release_dispatch() {
    local channel=$1 commit=$2 platforms request attempt
    platforms=${KODEM_RELEASE_PLATFORMS:-macos windows linux}
    # The workflow names its run after this identifier, which finds the run that this call starts.
    request="$(date -u +%Y%m%dT%H%M%SZ)-$$"
    gh workflow run "$release_workflow" --repo "$release_repository" \
        --ref "$(gh repo view "$release_repository" --json defaultBranchRef --jq .defaultBranchRef.name)" \
        -f channel="$channel" -f commit="$commit" -f platforms="$platforms" \
        -f version="${KODEM_VERSION:-}" -f request="$request" \
        || { echo "Could not start $release_workflow. It must be on the default branch on GitHub, and gh needs the workflow scope (gh auth refresh -s workflow)." >&2; return 1; }
    release_run=""
    for attempt in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
        release_run=$(gh run list --repo "$release_repository" --workflow "$release_workflow" --event workflow_dispatch --limit 20 --json databaseId,displayTitle --jq "[.[] | select(.displayTitle | endswith(\"$request\"))] | first | .databaseId // empty")
        [[ -z "$release_run" ]] || break
        sleep 3
    done
    [[ -n "$release_run" ]] || { echo "GitHub did not list the $release_workflow run that this command started." >&2; return 1; }
}

# Shows the workflow run's progress until it ends: each job's state, its current step and how
# many steps finished, whenever that changes, and the elapsed time every five minutes otherwise.
# Succeeds when the run succeeded.
kodem_release_wait() {
    local started now snapshot last="" reported=0 failures=0 state jobs
    started=$(date +%s)
    echo "Release workflow: https://github.com/$release_repository/actions/runs/$release_run"
    while true; do
        # The first line is the run's state; each further line describes one job.
        if ! snapshot=$(gh run view "$release_run" --repo "$release_repository" --json status,conclusion,jobs --jq '
            "\(.status) \(.conclusion // "")",
            (.jobs[] | "  \(.name): " + (
                if .status == "completed" then .conclusion
                else ({"in_progress": "running", "queued": "queued", "waiting": "waiting for a runner"}[.status] // .status)
                    + (([.steps[] | select(.status == "in_progress")][0].name // "") as $step
                    | if $step == "" then "" else " · " + $step end)
                end)
                + " (\([.steps[] | select(.status == "completed")] | length)/\(.steps | length) steps)")' 2>/dev/null); then
            failures=$((failures + 1))
            [[ "$failures" -lt 20 ]] || { echo "GitHub stopped answering about the workflow run." >&2; return 1; }
            sleep 30
            continue
        fi
        failures=0
        now=$(date +%s)
        if [[ "$snapshot" != "$last" || $((now - reported)) -ge 300 ]]; then
            jobs=$(printf '%s\n' "$snapshot" | sed 1d)
            [[ -n "$jobs" ]] || jobs="  waiting for GitHub to start the jobs"
            printf '[%s, %dm %02ds]\n%s\n' "$(date +%H:%M)" $(((now - started) / 60)) $(((now - started) % 60)) "$jobs"
            last=$snapshot
            reported=$now
        fi
        state=$(printf '%s\n' "$snapshot" | head -1)
        if [[ "$state" == completed* ]]; then
            [[ "$state" == "completed success" ]]
            return
        fi
        sleep 20
    done
}

# Checks the packages that the workflow's builds produced in a folder: every update archive and
# installer of the platforms in the second argument exists, and each checksum file matches its
# file. Lists them in release_assets.
kodem_release_verify_assets() {
    local folder=$1 platforms=$2 file expected platform
    release_assets=()
    for platform in $platforms; do
        case "$platform" in
            macos) expected="Kodem-macos-aarch64.zip Kodem-macos-aarch64.dmg" ;;
            windows) expected="Kodem-windows-x86_64.zip Kodem-windows-x86_64-setup.exe Kodem-windows-aarch64.zip Kodem-windows-aarch64-setup.exe" ;;
            linux) expected="Kodem-linux-x86_64.tar.gz Kodem-linux-x86_64-setup.run" ;;
            *) echo "Unknown platform $platform. Use macos, windows and linux." >&2; return 1 ;;
        esac
        for file in $expected; do
            [[ -n "$(find "$folder" -type f -name "$file")" ]] || { echo "The $platform build did not produce $file." >&2; return 1; }
            [[ -n "$(find "$folder" -type f -name "$file.sha256")" ]] || { echo "$file has no checksum file." >&2; return 1; }
        done
    done
    while IFS= read -r file; do
        if [[ "$file" == *.sha256 ]]; then
            [[ "$(cut -d ' ' -f 1 "$file")" == "$(kodem_sha256 "${file%.sha256}")" ]] || { echo "$(basename "$file") does not match its file." >&2; return 1; }
        fi
        release_assets+=("$file")
    done < <(find "$folder" -type f | sort)
}

# Publishes only after the upload of every file and GitHub's SHA-256 digests agree. Further
# files, such as other platforms' archives, checksums and signatures, go into the same release.
kodem_release_publish() {
    local channel=$1 title digest file asset release_api
    shift
    local files=("$@")
    local flags=(--latest)
    title="Kodem $release_version"
    if [[ "$channel" == nightly ]]; then title="Kodem Nightly $release_version"; flags=(--prerelease --latest=false); fi
    [[ "$(git rev-parse HEAD)" == "$release_commit" && -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Source changed during the build; refusing publication.' >&2; return 1; }
    if [[ -n "${release_existing:-}" ]]; then
        release_api=$(gh release view "$release_tag" --repo "$release_repository" --json apiUrl --jq .apiUrl)
        for file in "${files[@]}"; do
            if gh api "$release_api" --jq '.assets[].name' | grep -Fxq "$(basename "$file")"; then
                echo "Release $release_tag already has $(basename "$file"). Delete that asset on GitHub to replace it." >&2
                return 1
            fi
        done
        gh release upload "$release_tag" "${files[@]}" --repo "$release_repository"
    else
        # A failed upload leaves a draft, never a partial public release.
        gh release create "$release_tag" "${files[@]}" --repo "$release_repository" --target "$release_commit" --title "$title" --generate-notes --draft "${flags[@]}"
        # The REST tag endpoint cannot find an unpublished draft. The CLI resolves drafts too.
        release_api=$(gh release view "$release_tag" --repo "$release_repository" --json apiUrl --jq .apiUrl)
    fi
    for file in "${files[@]}"; do
        asset=$(basename "$file")
        digest=$(gh api "$release_api" --jq ".assets[] | select(.name == \"$asset\") | .digest")
        [[ "$digest" == "sha256:$(kodem_sha256 "$file")" ]] && continue
        if [[ -n "${release_existing:-}" ]]; then
            # Updaters must never see an archive that differs from the build.
            for file in "${files[@]}"; do
                gh release delete-asset "$release_tag" "$(basename "$file")" --repo "$release_repository" --yes || true
            done
            echo "Upload verification failed for $asset. This build's files were removed from $release_tag." >&2
        else
            echo "Upload verification failed for $asset. Release $release_tag remains a draft." >&2
        fi
        return 1
    done
    if [[ -n "${release_existing:-}" ]]; then
        echo "PUBLISHED: https://github.com/$release_repository/releases/tag/$release_tag"
    elif [[ "${KODEM_RELEASE_STAGE_ONLY:-0}" == 1 ]]; then
        # The release workflow cannot create release tags; an owner account publishes the draft.
        echo "STAGED: $(gh release view "$release_tag" --repo "$release_repository" --json url --jq .url)"
    else
        gh release edit "$release_tag" --repo "$release_repository" --draft=false "${flags[@]}"
        echo "PUBLISHED: https://github.com/$release_repository/releases/tag/$release_tag"
    fi
}

# Publishes the draft release that the release workflow staged for a commit, which creates its
# tag under this account. Every file must carry the SHA-256 digest that the workflow verified.
kodem_release_finish() {
    local channel=$1 commit=$2 tag missing
    local flags=(--latest)
    [[ "$channel" != nightly ]] || flags=(--prerelease --latest=false)
    tag=$(gh api "repos/$release_repository/releases?per_page=30" --jq "[.[] | select(.draft and .target_commitish == \"$commit\")] | first | .tag_name // empty")
    [[ -n "$tag" ]] || { echo "The workflow staged no draft release for $commit." >&2; return 1; }
    missing=$(gh release view "$tag" --repo "$release_repository" --json assets --jq '[.assets[] | select((.digest // "") | startswith("sha256:") | not) | .name] | join(", ")')
    [[ -z "$missing" ]] || { echo "Draft $tag has files without a verified digest: $missing" >&2; return 1; }
    gh release edit "$tag" --repo "$release_repository" --draft=false "${flags[@]}"
    echo "PUBLISHED: https://github.com/$release_repository/releases/tag/$tag"
}
