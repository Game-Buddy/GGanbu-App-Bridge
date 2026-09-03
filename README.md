# GGanbu App Bridge

GGanbu App Bridge is the native desktop companion for [GGanbu.app](https://gganbu.app). It provides an authenticated, origin-constrained local-network bridge between the browser experience and operating-system capabilities such as keyboard input and War Thunder keybinding mappings.

The latest stable release is available from [GitHub Releases](https://github.com/Game-Buddy/GGanbu-App-Bridge/releases/latest).

## What it does

- Connects GGanbu.app to the desktop bridge through an authenticated pairing flow.
- Resolves allowlisted actions and sends supported keyboard input through native platform adapters.
- Shows bridge health, activity, connected devices, and pairing state in the desktop UI.
- Loads the bundled War Thunder keybinding presets or imports a compatible mapping file.

## Availability

| Platform                              | Distribution                                                      | Status                         |
| ------------------------------------- | ----------------------------------------------------------------- | ------------------------------ |
| Ubuntu 24.04 x86_64 on X11 or Wayland | Signed AppImage                                                   | Available from GitHub Releases |
| Windows 11 x86_64                     | [Microsoft Store](https://apps.microsoft.com/detail/9nfnnsx3pc7j) | Available                      |
| macOS                                 | —                                                                 | Not currently planned          |

The supported platform list reflects the platforms currently built and tested by the project. Other Linux distributions may work, but are not part of the supported release matrix.

## Installation

### Linux

Download the AppImage and its verification files from the [latest release](https://github.com/Game-Buddy/GGanbu-App-Bridge/releases/latest). Verify the published checksum and detached signature before launching it. The complete procedure is in [release signing and verification](./docs/security/release-signing.md).

### Windows

Install GGanbu App Bridge from the [official Microsoft Store listing](https://apps.microsoft.com/detail/9nfnnsx3pc7j). GitHub Actions also produces an unsigned MSIX submission artifact for Partner Center; it is not a public installer and must not be installed or redistributed. The maintainer submission process is documented in [Microsoft Store distribution](./docs/windows-store.md).

Do not download installers from mirrors or third-party websites.

## First use

1. Launch GGanbu App Bridge.
2. Start the bridge server from the **Status** view, or choose **Add device** to start it while pairing.
3. Open [GGanbu.app](https://gganbu.app) in the browser.
4. Choose **Add device** in the bridge and enter the displayed pairing code in GGanbu.app.
5. Confirm that the device is connected before sending actions.

The bridge listens on port `53177`. If the browser cannot connect, check the bridge status, the configured browser origin, and any local-network permissions required by the operating system or browser.

## Development

GGanbu App Bridge is a Tauri 2 desktop application with a React frontend and a Rust local bridge service.

### AI-assisted development

Parts of this codebase may be developed with help from AI tools. AI-generated suggestions are reviewed, tested, and adapted by human contributors before they are included. Contributors remain responsible for the quality, security, licensing, and maintainability of their changes.

AI tools are used during development only; they are not part of the GGanbu App Bridge runtime and the application does not provide an AI service.

Contributing with AI assistance is welcome. The contributor who submits a change owns its correctness, its fit with the project, and everything that happens after it merges. See [CONTRIBUTING.md](./CONTRIBUTING.md) for the project’s contribution requirements.

### Prerequisites

- Node.js 24
- pnpm 11
- Rust 1.94.1
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for the host operating system
- War Thunder only when running game integration tests

### Run locally

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm tauri dev
```

The standard development server uses `http://127.0.0.1:5173` and `http://localhost:5173` by default. To use another browser origin, create a root `.env` file with a comma-separated list of exact origins:

```dotenv
GGANBU_BRIDGE_ALLOWED_ORIGINS=http://127.0.0.1:5173,http://localhost:5173
```

Release builds ignore that override and allow only `https://gganbu.app`. Never put secrets in `.env`; the file is intended for public origin configuration only.

### Run checks

```bash
pnpm check
pnpm check:workflows
```

See [CONTRIBUTING.md](./CONTRIBUTING.md) for branch, versioning, changelog, and pull-request requirements.

## Architecture and security

- [Application architecture](./docs/architecture.md) — source boundaries, request flow, and trust boundaries.
- [Security policy](./SECURITY.md) — private vulnerability reporting.
- [Release signing and verification](./docs/security/release-signing.md) — Linux signatures, checksums, and release provenance.
- [Repository governance](./docs/repository-governance.md) — protected branches, release controls, and maintainer settings.
- [War Thunder preset provenance](./docs/preset-provenance.md) — compatibility-data ownership and licensing context.

The WebView is treated as an untrusted client. Rust owns protocol validation, authentication, persistence, origin policy, and privileged operating-system input.

## Support and project information

- Use [GitHub Discussions](https://github.com/Game-Buddy/GGanbu-App-Bridge/discussions) for questions and support.
- Use the issue forms for reproducible bugs and actionable feature requests.
- See [CHANGELOG.md](./CHANGELOG.md) for release history.

## License

Copyright © 2026 Game Buddy.

GGanbu App Bridge is licensed under the [GNU Affero General Public License version 3 only](./LICENSE). Modified versions offered over a network must make their corresponding source available to the people using them.

The names and logos are subject to the [branding policy](./TRADEMARKS.md). Third-party components retain their own licenses as listed in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
