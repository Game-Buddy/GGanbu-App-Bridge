# Repository governance

This document records GitHub settings that complement the versioned files in the public repository. Maintainers should review it whenever ownership, CI, or the release process changes.

## Current public baseline

- Keep the public history limited to reviewed, intended content. Do not publish private refs, credentials, or unreviewed snapshots.
- Keep the repository public.
- Keep Issues and Discussions enabled.
- Keep the wiki disabled; documentation belongs in version control.
- Keep `main` as the default, integration, and stable release branch.
- Allow the configured merge method for pull requests into `main`.
- Automatically delete merged feature branches.
- Set default GitHub Actions permissions to read-only and do not allow Actions to create or approve pull requests.

## Branch rulesets

Keep an active ruleset for `main` that:

- requires pull requests;
- requires at least one approving review;
- dismisses stale approvals when the diff changes;
- requires all conversations to be resolved;
- blocks force pushes and deletion;
- requires the `required-ci` check;
- requires branches to be up to date before merging;
- applies to administrators unless an emergency bypass process is documented.

The `required-ci` check collects the application, security, and platform results while allowing intentionally skipped jobs.

Use short-lived feature branches from `main`. After a release-ready pull request is merged, create the version tag on the resulting `main` commit.

## Public history and release assets

Before publishing a new public ref or release asset, verify that it contains only reviewed, intended content. A current-tree deletion does not remove sensitive material from earlier commits; treat exposed credentials or private data as compromised and rotate or revoke them.

Keep a separate active tag ruleset for version tags matching `v*` that restricts tag creation, blocks updates, force pushes, and deletion. A workflow tag trigger does not protect a tag.

## Security settings

Enable and regularly review:

- Dependabot alerts and security updates;
- secret scanning and push protection;
- CodeQL default or advanced setup, without duplicating the committed workflow;
- private vulnerability reporting;
- dependency review on pull requests.

Dependabot covers GitHub Actions, pnpm dependencies at `/`, and Cargo dependencies at `/src-tauri`.

Do not add repository or environment secrets until their actual values and owners are known. Never store private signing keys, tokens, or credentials in source, local configuration committed to Git, workflow artifacts, or issue content.

## Release environment

Maintain a protected environment named `release`. Add known maintainer reviewers, prevent self-review, limit it to protected release tags, and store release-only credentials there. Follow [security/release-signing.md](./security/release-signing.md) for the exact names and activation checklist.

## Deferred ownership decisions

Add the following only when the responsible people and stable values are known:

- review the existing `.github/CODEOWNERS` configuration and confirm `@GGanbu-App` team membership;
- repository collaborators and teams;
- release environment reviewers;
- required status-check names;
- Actions variables and secrets;
- organization-wide Actions and security policies.
