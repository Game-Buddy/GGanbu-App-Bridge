# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.9] - 2026-08-23

### Fixed

- Select the x64 Windows SDK signing tool, bind signing to the imported certificate, and verify the installer signature before publishing.
- Document the trusted-main release trigger and keep pinned CodeQL action metadata exact.

## [0.4.8] - 2026-08-23

### Fixed

- Query the owning repository explicitly when the signed release workflow verifies completed CI.

## [0.4.7] - 2026-08-23

### Added

- Add focused Tauri commands, application modules, mappings, and architecture tests.

### Changed

- Refactor bridge, server, keyboard, pairing, and security state handling into clearer Rust modules.
- Gate signed releases on successful CI and CodeQL, then tag and publish only after artifact signing.

## [0.4.6] - 2026-08-23

### Fixed

- Restore crypto dependency versions compatible with the OPAQUE implementation.
- Keep the version rollback path compliant with ESLint's caught-error rules.
- Keep TypeScript on the version supported by the configured `typescript-eslint` release.

## [0.4.5] - 2026-08-23

### Added

- Require every pull request merged into `main` to update the application version and changelog.

### Changed

- Run the signed desktop release workflow from the merged `main` version and skip versions that already have a GitHub Release.

## [0.4.4] - 2026-08-21

### Fixed

- Keep the status and activity layouts readable at the 16:9 minimum window size.
- Enforce the 960×540 minimum native window size and give Activity more room for Action IDs.

## [0.4.3] - 2026-08-21

### Fixed

- Ensure local optimized builds load the bundled `.env` origin allowlist while tagged release builds retain the static production policy.
- Allow configured local origins such as `http://localhost:5177` to pass bridge CORS checks in local builds.

## [0.4.2] - 2026-08-21

### Changed

- Read the development origin allowlist from `GGANBU_BRIDGE_ALLOWED_ORIGINS` in `.env`.
- Use the static `https://gganbu.app` origin allowlist in release builds.
- Include the origin policy behavior in the bridge build documentation.

## [0.4.1] - 2026-08-21

### Changed

- Updated frontend and Rust dependency versions while retaining compatibility with the existing OPAQUE and RNG integrations.
- Updated pinned release workflow actions for artifact downloads and build attestations.
- Grouped npm, Cargo, and GitHub Actions Dependabot updates into one weekly pull request.

## [0.4.0] - 2026-08-20

### Added

- Initial public repository scaffold, contribution guidance, security policy, and community templates.
- Tauri 2 desktop application with a React frontend and Rust local bridge service.
- Local bridge server controls and live status reporting in the desktop UI.
- Secure pairing and authenticated message flows using the OPAQUE protocol and encrypted envelopes.
- Action catalog and War Thunder hotkey mappings for keyboard, gamepad, joystick, and simulator presets.
- Platform-aware native keyboard execution for Windows, X11, and Wayland environments.
- JavaScript and Rust quality checks, CodeQL analysis, dependency auditing, and signed release workflow configuration.

### Changed

- Consolidated bridge state, action history, pairing status, and server lifecycle updates into the desktop UI.
- Added persistent device security state and guarded bridge request handling.

### Security

- Added authenticated pairing, encrypted bridge payloads, replay protection, request validation, and bounded request bodies.
- Added release-signing documentation and verification workflow configuration.

[unreleased]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.8...HEAD
[0.4.8]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.7...v0.4.8
[0.4.7]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.6...v0.4.7
[0.4.6]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.5...v0.4.6
[0.4.5]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.4...v0.4.5
[0.4.4]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/Game-Buddy/GGanbu-App-Bridge/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Game-Buddy/GGanbu-App-Bridge/releases/tag/v0.4.0
