#!/bin/bash
# Shared release metadata and publication steps. Sourced by build-macos.sh.
release_repository=adamwawrzynkowski/kodem-ade

# Resolves a monotonically increasing channel version before compiling.
kodem_release_prepare() {
    local channel=$1 base tags previous candidate
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
    tags=$(gh api --paginate "repos/$release_repository/releases?per_page=100" --jq '.[].tag_name')
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

# Publishes only after the archive upload and GitHub SHA-256 digest agree.
kodem_release_publish() {
    local channel=$1 archive=$2 title digest local_digest release_id
    local flags=(--latest)
    title="Kodem $release_version"
    if [[ "$channel" == nightly ]]; then title="Kodem Nightly $release_version"; flags=(--prerelease --latest=false); fi
    [[ "$(git rev-parse HEAD)" == "$release_commit" && -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Source changed during the build; refusing publication.' >&2; return 1; }
    # A failed upload leaves a draft, never a partial public release.
    gh release create "$release_tag" "$archive" --repo "$release_repository" --target "$release_commit" --title "$title" --generate-notes --draft "${flags[@]}"
    release_id=$(gh api "repos/$release_repository/releases/tags/$release_tag" --jq .id)
    digest=$(gh api "repos/$release_repository/releases/$release_id" --jq '.assets[] | select(.name == "Kodem-macos-aarch64.zip") | .digest')
    local_digest="sha256:$(/usr/bin/shasum -a 256 "$archive" | cut -d ' ' -f 1)"
    [[ "$digest" == "$local_digest" ]] || { echo "Upload verification failed. Release $release_tag remains a draft." >&2; return 1; }
    gh release edit "$release_tag" --repo "$release_repository" --draft=false "${flags[@]}"
    echo "PUBLISHED: https://github.com/$release_repository/releases/tag/$release_tag"
}
