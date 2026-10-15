#!/bin/bash
# Releases Kodem for every platform through the Release packages workflow.
#
#   release.sh nightly|production    On your computer: starts the workflow for the checked-out
#                                    commit, shows its progress, and publishes the draft release
#                                    that it staged. Closing the window leaves the workflow running;
#                                    publish its draft on GitHub later.
#   release.sh plan CHANNEL          In the workflow: chooses the version and the change list.
#   release.sh publish CHANNEL DIR   In the workflow: verifies the packages in DIR and stages them
#                                    as one draft release.
#
# The workflow cannot create release tags, which a ruleset reserves for the owner's accounts, so
# a release becomes public when one of them publishes the staged draft.
#
# KODEM_VERSION chooses a newer version explicitly. KODEM_RELEASE_PLATFORMS limits the build to
# some of macos, windows and linux.
set -euo pipefail
KODEM_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$KODEM_ROOT"
source "$KODEM_ROOT/scripts/dependencies.sh"
source "$KODEM_ROOT/scripts/github-release.sh"

usage() {
    echo "Usage: $0 nightly|production | plan CHANNEL | publish CHANNEL DIR" >&2
    exit 2
}

# Checks a release channel name.
channel_argument() {
    [[ "${1:-}" == nightly || "${1:-}" == production ]] || usage
}

# Starts the workflow for this checkout's commit and follows it.
start() {
    local channel=$1 commit url
    command -v gh >/dev/null || { echo 'Install GitHub CLI and run gh auth login.' >&2; exit 1; }
    gh auth status >/dev/null 2>&1 || { echo 'Run gh auth login with the owner or a collaborator account.' >&2; exit 1; }
    kodem_release_check_account
    [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'Commit your changes first. The workflow builds the commit that is checked out here.' >&2; exit 1; }
    commit=$(git rev-parse HEAD)
    [[ "$(gh api "repos/$release_repository/commits/$commit" --jq .sha 2>/dev/null)" == "$commit" ]] || { echo "Push $commit to GitHub first. The workflow builds it there." >&2; exit 1; }
    kodem_release_dispatch "$channel" "$commit"
    echo "Releasing $channel for $commit in GitHub Actions. Closing this window leaves the release running there."
    if ! kodem_release_wait; then
        echo "The release did not finish. Nothing was published. See https://github.com/$release_repository/actions/runs/$release_run" >&2
        exit 1
    fi
    kodem_release_finish "$channel" "$commit"
}

# Writes the version, tag, commit and change list as outputs of the workflow's plan job.
plan() {
    local channel=$1 delimiter
    kodem_release_prepare "$channel"
    [[ -z "$release_existing" ]] || { echo "This commit is already released as $release_tag." >&2; exit 1; }
    # A random delimiter cannot appear in the commit subjects that the list holds.
    delimiter="KODEM_$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
    {
        echo "version=$release_version"
        echo "tag=$release_tag"
        echo "commit=$release_commit"
        echo "previous=$release_previous"
        echo "notes<<$delimiter"
        printf '%s\n' "$release_notes"
        echo "$delimiter"
    } >> "${GITHUB_OUTPUT:?plan runs in the release workflow}"
}

# Stages the packages that the workflow's builds left in a folder as a draft release.
publish() {
    local channel=$1 folder=$2
    [[ -n "${KODEM_RELEASE_VERSION:-}" ]] || { echo 'publish runs in the release workflow, after plan.' >&2; exit 1; }
    kodem_release_prepare "$channel"
    kodem_release_verify_assets "$folder" "${KODEM_RELEASE_PLATFORMS:-macos windows linux}"
    kodem_release_publish "$channel" "${release_assets[@]}"
}

case "${1:-}" in
    nightly|production) start "$1" ;;
    plan) channel_argument "${2:-}"; plan "$2" ;;
    publish) channel_argument "${2:-}"; [[ -d "${3:-}" ]] || usage; publish "$2" "$3" ;;
    *) usage ;;
esac
