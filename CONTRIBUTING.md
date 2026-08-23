# Contributing to GGanbu App Bridge

Thanks for helping improve GGanbu App Bridge. The repository is currently being bootstrapped, so discuss large product, protocol, or architecture changes before investing in an implementation.

## Before you start

- Search existing issues and discussions.
- Use Discussions for support and early design questions.
- Open an issue for reproducible bugs or a scoped feature proposal.
- Report security issues privately as described in [SECURITY.md](./SECURITY.md).

## Branches

1. Branch from `main` using a short descriptive name such as `feature/pairing-status` or `fix/linux-startup`.
2. Keep the branch focused on one change.
3. Open the pull request against `main` and complete the pull-request template.
4. Merge the pull request into `main` after the required checks pass.
5. After the merged `main` commit passes `Checks and desktop builds` and
   `CodeQL`, the release workflow starts when its version has not already been
   released. The protected release environment controls approval, signing, tag
   creation, and publication.

Direct pushes to `main` are not part of the normal workflow.

## Local checks

Install the repository-managed pre-commit hook once after cloning:

```bash
pnpm install --frozen-lockfile
pnpm hooks:install
```

The hook runs `pnpm check`, covering formatting, linting, type checking, frontend tests/build, and Rust checks. Run `pnpm check` manually when needed; the desktop bundle build remains a separate CI/release check.

## Versioning

`VERSION` is the source of truth for the application version. To change it, run:

```bash
pnpm version:set 0.4.0
```

This updates the package, Cargo, Tauri, and frontend version files. Run `pnpm version:check` to verify that all application versions remain synchronized. Update `CHANGELOG.md` in the same change.

## Pull requests

- Explain the user-visible outcome and why the change is needed.
- Link related issues with `Closes #123` when appropriate.
- Include testing evidence and screenshots for UI changes.
- Add or update tests for behavior changes.
- Update the application version with `pnpm version:set` for every pull request.
- Update documentation and `CHANGELOG.md` for user-visible changes.
- Do not include generated build outputs, secrets, signing keys, tokens, or credentials.
- Resolve review conversations before requesting final approval.

Pull requests into `main` use the repository's configured merge method. By contributing, you agree to license your contribution under `AGPL-3.0-only`. Keep third-party material out of contributions unless its license is compatible and its copyright and license notices are included.
