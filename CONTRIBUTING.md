# Contributing to GGanbu App Bridge

Thanks for helping improve GGanbu App Bridge. For support or early design questions, use [GitHub Discussions](https://github.com/Game-Buddy/GGanbu-App-Bridge/discussions). Open an issue for a reproducible bug or a focused feature request. Report security issues privately through [SECURITY.md](./SECURITY.md).

## Get started

1. Create a short-lived branch from `main`, for example `feature/pairing-status` or `fix/linux-startup`.
2. Install the dependencies and enable the repository hook:

   ```bash
   pnpm install --frozen-lockfile
   pnpm hooks:install
   ```

3. Run the application with `pnpm tauri dev`.
4. Run `pnpm check` before opening a pull request. Workflow checks can be run with `pnpm check:workflows` when the required tools are installed.

The full check includes formatting, linting, type checking, frontend tests and build, Rust dependency analysis, and Rust checks. CI runs the required checks again.

## Version and changelog

Every pull request must update the application version and `CHANGELOG.md`:

```bash
pnpm version:set 0.6.0
pnpm version:check
```

`VERSION` is the source of truth. The version command keeps the package, Cargo, Tauri, and frontend versions synchronized.

## Pull requests

- Target `main` and keep the change focused.
- Explain the user-visible result and link related issues when appropriate.
- Add or update tests for behavior changes.
- Include testing evidence and screenshots for UI changes.
- Update documentation for user-visible behavior.
- Do not commit generated outputs, secrets, signing keys, tokens, or credentials.
- Resolve review conversations before requesting final approval.

Pull requests are merged after the required checks and review pass. By contributing, you agree to license your contribution under `AGPL-3.0-only`. Keep third-party material out of contributions unless its license and copyright notices are included.
