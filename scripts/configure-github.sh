#!/bin/bash
# Applies the repository's GitHub protection settings. Safe to run again: it updates the existing
# rulesets by name. Needs GitHub CLI signed in as a repository administrator.
#
# - main: only the owner and collaborators with write access can push or merge; pull requests from
#   anyone else need the code owner's approval, resolved threads and the Checks workflow; no force
#   pushes or deletion.
# - Release tags (v*, nightly/v*): only the owner and collaborators with write access can create,
#   move or delete them, so nobody else can publish a release that installed apps would download.
#   A repository of a personal account cannot make a collaborator an administrator, so the write
#   role bypasses both rulesets: add as collaborators only accounts that may push to main and
#   publish releases.
#   The Release packages workflow cannot create these tags: it stages a draft release, and an
#   owner account publishes it, which creates the tag.
# - Actions: read-only token by default, GitHub's own actions, the pinned cache action and
#   SignPath's signing action only, actions pinned to full commit SHAs, approval for every
#   outside contributor's workflow run. GitHub checks the list when it loads a workflow, so the
#   release workflow needs the SignPath action allowed even while SignPath is not configured.
# - Security: secret scanning with push protection, Dependabot alerts, private vulnerability
#   reporting.
set -euo pipefail
repository=${1:-adamwawrzynkowski/kodem-ade}
api="repos/$repository"
# GitHub's numbers for the repository write and administrator roles and the GitHub Actions app.
write_role=4
admin_role=5
actions_app=15368

[[ "$(gh api "$api" --jq .permissions.admin)" == true ]] || { echo "GitHub CLI must be signed in as an administrator of $repository." >&2; exit 1; }
echo "Configuring $repository as $(gh api user --jq .login)"

# Creates the ruleset with this name, or replaces the existing one's settings.
apply_ruleset() {
    local name=$1 body=$2 id
    id=$(gh api "$api/rulesets" --jq ".[] | select(.name == \"$name\") | .id")
    if [[ -n "$id" ]]; then
        gh api -X PUT "$api/rulesets/$id" --input - <<< "$body" >/dev/null
        echo "Updated ruleset: $name"
    else
        gh api -X POST "$api/rulesets" --input - <<< "$body" >/dev/null
        echo "Created ruleset: $name"
    fi
}

apply_ruleset Main "$(cat <<JSON
{
  "name": "Main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [
    { "actor_id": $admin_role, "actor_type": "RepositoryRole", "bypass_mode": "always" },
    { "actor_id": $write_role, "actor_type": "RepositoryRole", "bypass_mode": "always" }
  ],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "creation" },
    { "type": "update" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 1,
        "dismiss_stale_reviews_on_push": true,
        "require_code_owner_review": true,
        "require_last_push_approval": true,
        "required_review_thread_resolution": true,
        "require_extra_approval_for_unattributed_changes": true,
        "allowed_merge_methods": ["squash", "rebase"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": true,
        "required_status_checks": [
          { "context": "Format and licenses", "integration_id": $actions_app },
          { "context": "Clippy (macos-14)", "integration_id": $actions_app },
          { "context": "Clippy (windows-2022)", "integration_id": $actions_app },
          { "context": "Clippy (ubuntu-22.04)", "integration_id": $actions_app }
        ]
      }
    },
    { "type": "copilot_code_review", "parameters": { "review_on_push": false, "review_draft_pull_requests": false } }
  ]
}
JSON
)"

apply_ruleset "Release tags" "$(cat <<JSON
{
  "name": "Release tags",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/v*", "refs/tags/nightly/v*"], "exclude": [] } },
  "bypass_actors": [
    { "actor_id": $admin_role, "actor_type": "RepositoryRole", "bypass_mode": "always" },
    { "actor_id": $write_role, "actor_type": "RepositoryRole", "bypass_mode": "always" }
  ],
  "rules": [
    { "type": "creation" },
    { "type": "update" },
    { "type": "deletion" }
  ]
}
JSON
)"

gh api -X PATCH "$api" --input - >/dev/null <<'JSON'
{
  "has_wiki": false,
  "delete_branch_on_merge": true,
  "allow_merge_commit": false,
  "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" }
  }
}
JSON
echo "Enabled secret scanning and push protection"
gh api -X PUT "$api/vulnerability-alerts" >/dev/null
echo "Enabled Dependabot alerts"
gh api -X PUT "$api/private-vulnerability-reporting" >/dev/null
echo "Enabled private vulnerability reporting"

gh api -X PUT "$api/actions/permissions" --input - >/dev/null <<'JSON'
{ "enabled": true, "allowed_actions": "selected", "sha_pinning_required": true }
JSON
gh api -X PUT "$api/actions/permissions/selected-actions" --input - >/dev/null <<'JSON'
{ "github_owned_allowed": true, "verified_allowed": false, "patterns_allowed": ["Swatinem/rust-cache@*", "signpath/github-action-submit-signing-request@*"] }
JSON
gh api -X PUT "$api/actions/permissions/workflow" --input - >/dev/null <<'JSON'
{ "default_workflow_permissions": "read", "can_approve_pull_request_reviews": false }
JSON
gh api -X PUT "$api/actions/permissions/fork-pr-contributor-approval" --input - >/dev/null <<'JSON'
{ "approval_policy": "all_external_contributors" }
JSON
echo "Restricted GitHub Actions"
echo "Done."
