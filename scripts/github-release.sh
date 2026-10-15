#!/bin/bash
# Shared release metadata and publication steps. Sourced by the macOS, Linux and Windows builds.
# Builds of one commit on several platforms share one release; each adds its own archive.
#
# The macOS release commands publish every platform at once: they start the Release packages
# workflow (.github/workflows/release.yml) for Windows and Linux, build macOS meanwhile, collect
# the workflow's packages and publish one release with all of them. The workflow's builds run
# with KODEM_RELEASE_VERSION set, which fixes the version and leaves publication to the caller.
release_repository=adamwawrzynkowski/kodem-ade
release_workflow=release.yml

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
    gh auth status >/dev/null 2>&1 || { echo 'Run gh auth login with the owner or a collaborator account.' >&2; return 1; }
    # The Release tags ruleset reserves release tags for the owner and collaborators. Check before
    # building, not at the upload.
    [[ "$(gh api "repos/$release_repository" --jq .permissions.push 2>/dev/null)" == true ]] || { echo "GitHub CLI is signed in as $(gh api user --jq .login 2>/dev/null || echo 'an unknown account'), which cannot create release tags. Run gh auth switch to the owner or a collaborator account." >&2; return 1; }
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

# Tells whether this build publishes its packages itself.
kodem_release_publishes() {
    [[ "$1" != debug && -z "${KODEM_RELEASE_VERSION:-}" ]]
}

# Starts the Release packages workflow for the platforms in KODEM_RELEASE_PLATFORMS (default:
# "windows linux"; empty builds macOS only) and sets release_run to its run. An existing release
# of this commit gets no workflow: the per-platform release commands add to it.
kodem_release_dispatch() {
    local channel=$1 platforms title attempt
    platforms=${KODEM_RELEASE_PLATFORMS-windows linux}
    release_run=""
    [[ -n "$platforms" ]] || { echo "Building macOS only, as KODEM_RELEASE_PLATFORMS is empty."; return 0; }
    if [[ -n "${release_existing:-}" ]]; then
        echo "$release_tag already exists, so this build adds macOS only. Run the Windows and Linux release commands to add theirs."
        return 0
    fi
    # The workflow names its run after these inputs, which finds the run that this call starts.
    title="Release $channel $release_version at $release_commit"
    gh workflow run "$release_workflow" --repo "$release_repository" \
        --ref "$(gh repo view "$release_repository" --json defaultBranchRef --jq .defaultBranchRef.name)" \
        -f channel="$channel" -f version="$release_version" -f commit="$release_commit" -f platforms="$platforms" \
        -f previous="$release_previous" -f notes="$release_notes" \
        || { echo "Could not start $release_workflow. It must be on the default branch on GitHub, and gh needs the workflow scope (gh auth refresh -s workflow)." >&2; return 1; }
    for attempt in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
        release_run=$(gh run list --repo "$release_repository" --workflow "$release_workflow" --event workflow_dispatch --limit 20 --json databaseId,displayTitle --jq "[.[] | select(.displayTitle == \"$title\")] | first | .databaseId // empty")
        [[ -z "$release_run" ]] || break
        sleep 3
    done
    [[ -n "$release_run" ]] || { echo "GitHub did not list the $release_workflow run that this build started." >&2; return 1; }
    echo "Building $platforms in GitHub Actions: https://github.com/$release_repository/actions/runs/$release_run"
}

# Waits for the workflow run, downloads its packages into a folder, checks their checksum files
# and the expected archives, and lists them in release_assets.
kodem_release_collect() {
    local folder=$1 file expected platform
    release_assets=()
    [[ -n "${release_run:-}" ]] || return 0
    echo "Waiting for the Windows and Linux builds…"
    if ! gh run watch "$release_run" --repo "$release_repository" --exit-status --interval 30 >/dev/null; then
        echo "The Windows or Linux build failed. Nothing was published. See https://github.com/$release_repository/actions/runs/$release_run" >&2
        return 1
    fi
    rm -rf "$folder"
    mkdir -p "$folder"
    gh run download "$release_run" --repo "$release_repository" --dir "$folder"
    for platform in ${KODEM_RELEASE_PLATFORMS-windows linux}; do
        case "$platform" in
            windows) expected="Kodem-windows-x86_64.zip Kodem-windows-aarch64.zip" ;;
            linux) expected="Kodem-linux-x86_64.tar.gz" ;;
            *) echo "KODEM_RELEASE_PLATFORMS lists $platform. Use windows, linux or both." >&2; return 1 ;;
        esac
        for file in $expected; do
            [[ -n "$(find "$folder" -type f -name "$file")" ]] || { echo "The workflow did not produce $file." >&2; return 1; }
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
    if [[ -z "${release_existing:-}" ]]; then
        gh release edit "$release_tag" --repo "$release_repository" --draft=false "${flags[@]}"
    fi
    echo "PUBLISHED: https://github.com/$release_repository/releases/tag/$release_tag"
}
