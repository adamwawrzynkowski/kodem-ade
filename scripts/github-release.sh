#!/bin/bash
# Shared release metadata and publication steps. Sourced by the macOS, Linux and Windows builds.
# Builds of one commit on several platforms share one release; each adds its own archive.
release_repository=adamwawrzynkowski/kodem-ade

# Resolves a monotonically increasing channel version before compiling, or the version of the
# release that another platform already published for this commit.
kodem_release_prepare() {
    local channel=$1 base tags previous candidate releases pattern tag target draft
    base=$(cargo pkgid -p kodem-desktop)
    base=${base##*#}
    base=${base##*@}
    release_version=$base
    [[ "$channel" != debug ]] || return 0
    command -v gh >/dev/null || { echo 'Install GitHub CLI and run gh auth login before publishing.' >&2; return 1; }
    gh auth status >/dev/null 2>&1 || { echo 'Run gh auth login with repository write access.' >&2; return 1; }
    [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Release builds require a clean committed checkout. No changes are committed or pushed by this script.' >&2; return 1; }
    release_commit=$(git rev-parse HEAD)
    [[ "$(gh api "repos/$release_repository/commits/$release_commit" --jq .sha)" == "$release_commit" ]] || { echo 'The build commit must already exist on GitHub.' >&2; return 1; }
    releases=$(gh api --paginate "repos/$release_repository/releases?per_page=100" --jq '.[] | [.tag_name, .target_commitish, (.draft | tostring)] | @tsv')
    tags=$(printf '%s\n' "$releases" | cut -f 1)
    pattern='^v[0-9]+\.[0-9]+\.[0-9]+$'
    [[ "$channel" != nightly ]] || pattern='^nightly/v[0-9]+\.[0-9]+\.[0-9]+$'
    release_existing=""
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
}

# Publishes only after the archive upload and GitHub SHA-256 digest agree. Further files, such
# as checksums and signatures, are uploaded with the archive.
kodem_release_publish() {
    local channel=$1 archive=$2 title digest local_digest release_api asset
    shift 2
    local extras=("$@")
    local flags=(--latest)
    asset=$(basename "$archive")
    title="Kodem $release_version"
    if [[ "$channel" == nightly ]]; then title="Kodem Nightly $release_version"; flags=(--prerelease --latest=false); fi
    [[ "$(git rev-parse HEAD)" == "$release_commit" && -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Source changed during the build; refusing publication.' >&2; return 1; }
    if [[ -n "${release_existing:-}" ]]; then
        release_api=$(gh release view "$release_tag" --repo "$release_repository" --json apiUrl --jq .apiUrl)
        if gh api "$release_api" --jq '.assets[].name' | grep -Fxq "$asset"; then
            echo "Release $release_tag already has $asset. Delete that asset on GitHub to replace it." >&2
            return 1
        fi
        gh release upload "$release_tag" "$archive" ${extras[@]+"${extras[@]}"} --repo "$release_repository"
    else
        # A failed upload leaves a draft, never a partial public release.
        gh release create "$release_tag" "$archive" ${extras[@]+"${extras[@]}"} --repo "$release_repository" --target "$release_commit" --title "$title" --generate-notes --draft "${flags[@]}"
        # The REST tag endpoint cannot find an unpublished draft. The CLI resolves drafts too.
        release_api=$(gh release view "$release_tag" --repo "$release_repository" --json apiUrl --jq .apiUrl)
    fi
    digest=$(gh api "$release_api" --jq ".assets[] | select(.name == \"$asset\") | .digest")
    local_digest="sha256:$(kodem_sha256 "$archive")"
    if [[ "$digest" != "$local_digest" ]]; then
        if [[ -n "${release_existing:-}" ]]; then
            # Updaters on this platform must never see an archive that differs from the build.
            gh release delete-asset "$release_tag" "$asset" --repo "$release_repository" --yes || true
            echo "Upload verification failed. $asset was removed from $release_tag." >&2
        else
            echo "Upload verification failed. Release $release_tag remains a draft." >&2
        fi
        return 1
    fi
    if [[ -z "${release_existing:-}" ]]; then
        gh release edit "$release_tag" --repo "$release_repository" --draft=false "${flags[@]}"
    fi
    echo "PUBLISHED: https://github.com/$release_repository/releases/tag/$release_tag"
}
