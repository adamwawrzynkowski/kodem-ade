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
    # The commit subjects since the previous release of the channel, embedded for the app's
    # What's new dialog. Debug builds have none.
    release_previous=""
    release_notes=""
    [[ "$channel" != debug ]] || return 0
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
}

# Sets release_previous to the newest release of the channel before release_version, and
# release_notes to the subjects of the commits since it, newest first, without merges. GitHub
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
        || ! release_notes=$(gh api "repos/$release_repository/compare/$base...$release_commit?per_page=250" --jq '[.commits[] | select((.parents | length) < 2) | .commit.message | split("\n")[0]] | reverse | .[:100] | .[]'); then
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

# Checks the packages that the workflow's builds produced in a folder: every archive of the
# platforms in the second argument exists, and each checksum file matches its archive. Lists
# them in release_assets.
kodem_release_verify_assets() {
    local folder=$1 platforms=$2 file expected platform
    release_assets=()
    for platform in $platforms; do
        case "$platform" in
            macos) expected="Kodem-macos-aarch64.zip" ;;
            windows) expected="Kodem-windows-x86_64.zip Kodem-windows-aarch64.zip" ;;
            linux) expected="Kodem-linux-x86_64.tar.gz" ;;
            *) echo "Unknown platform $platform. Use macos, windows and linux." >&2; return 1 ;;
        esac
        for file in $expected; do
            [[ -n "$(find "$folder" -type f -name "$file")" ]] || { echo "The $platform build did not produce $file." >&2; return 1; }
            [[ -n "$(find "$folder" -type f -name "$file.sha256")" ]] || { echo "$file has no checksum file." >&2; return 1; }
        done
    done
    while IFS= read -r file; do
        if [[ "$file" == *.sha256 ]]; then
            [[ "$(cut -d ' ' -f 1 "$file")" == "$(kodem_sha256 "${file%.sha256}")" ]] || { echo "$(basename "$file") does not match its archive." >&2; return 1; }
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
