# GGanbu App Bridge

GGanbu App Bridge is the future public home of the GGanbu desktop companion. The application will provide secure local communication between the GGanbu.app browser experience and native operating-system capabilities.

> [!IMPORTANT]
> This repository contains the working desktop bridge application. Signed installers and a stable public release are still pending.

## Planned platforms

- Windows 11 x86_64
- Ubuntu 24.04 x86_64 on X11 and Wayland

macOS support is not currently planned. The supported platform list will be updated as builds are verified.

## Installation

There is no installable release yet. When the first signed build is published, download it from [GitHub Releases](https://github.com/Game-Buddy/GGanbu-App-Bridge/releases/latest). Release assets will be platform-specific and accompanied by `SHA256SUMS` and detached signatures.

Do not download installers from mirrors or third-party websites.

## Quick start for contributors

The application source is a Tauri 2 desktop app with a React frontend and Rust local bridge service:

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm tauri dev
```

Development requires Node.js 24, pnpm 11, Rust 1.94.1, the Tauri prerequisites for the host operating system, and War Thunder for end-to-end integration testing.

Development and local builds read `GGANBU_BRIDGE_ALLOWED_ORIGINS` from `.env` as a comma-separated list of exact browser origins. Local bundles include that public `.env` configuration. Release builds set `GGANBU_RELEASE_BUILD` in CI and use only the static `https://gganbu.app` origin; do not put secrets in `.env`.

See [CONTRIBUTING.md](./CONTRIBUTING.md) before opening a pull request.

The Tauri source layout, trust boundaries, and request flow are documented in
[docs/architecture.md](./docs/architecture.md).

## Repository workflow

- `main` is the default branch and the stable release branch.
- Feature and fix branches are short-lived and branch from `main`.
- Feature pull requests target `main`, must pass the required checks, and must update the synchronized application version and `CHANGELOG.md`.
- A push to `main` checks the version in the merged commit. If that version has no GitHub Release, the approved workflow creates its protected semantic version tag, builds the desktop packages, and publishes the signed release.

Pushes to `main` also produce QA packages. Stable installers come only from signed GitHub Releases created by the approved release workflow.

The maintainer-side GitHub settings that cannot be enforced by committed files are tracked in [docs/repository-governance.md](./docs/repository-governance.md).

## Security

Please do not disclose vulnerabilities in a public issue. Follow the private reporting instructions in [SECURITY.md](./SECURITY.md). Release verification and signing setup are documented in [docs/security/release-signing.md](./docs/security/release-signing.md).

## Troubleshooting

### No release is available

The project is still being bootstrapped. Watch the [Releases page](https://github.com/Game-Buddy/GGanbu-App-Bridge/releases) rather than installing an unofficial build.

### A workflow is skipped

Application CI runs the JavaScript and Rust quality checks and desktop build checks on supported branches.

### I need help or want to propose an idea

Use [GitHub Discussions](https://github.com/Game-Buddy/GGanbu-App-Bridge/discussions) for support and product questions. Use the issue forms for reproducible bugs and actionable feature requests.

## Project status and screenshots

The product UI is included in the desktop application. Screenshots will be added alongside the first signed public release.

## License

Copyright © 2026 Game Buddy.

GGanbu App Bridge is licensed under the [GNU Affero General Public License version 3 only](./LICENSE). Modified versions offered over a network must make their corresponding source available to the people using them.

The names and logos are subject to the [branding policy](./TRADEMARKS.md). Third-party components retain their own licenses as listed in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
