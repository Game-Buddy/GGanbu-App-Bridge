# Official Sources for Version-Sensitive Decisions

Use the project lockfile and manifests to identify exact versions before reading API documentation. Rust, Tauri, plugins, Tokio, and frontend packages change over time. Prefer documentation matching the installed version.

## Tauri

Primary documentation:

- Tauri 2 documentation: https://v2.tauri.app/
- Architecture: https://v2.tauri.app/concept/architecture/
- Process model: https://v2.tauri.app/concept/process-model/
- IPC overview: https://v2.tauri.app/concept/inter-process-communication/
- Calling Rust from the frontend: https://v2.tauri.app/develop/calling-rust/
- State management: https://v2.tauri.app/develop/state-management/
- Security overview: https://v2.tauri.app/security/
- Permissions: https://v2.tauri.app/security/permissions/
- Command scopes: https://v2.tauri.app/security/scope/
- Capabilities: https://v2.tauri.app/security/capabilities/
- CSP: https://v2.tauri.app/security/csp/
- Configuration reference: https://v2.tauri.app/reference/config/
- Testing: https://v2.tauri.app/develop/tests/
- Updater plugin: https://v2.tauri.app/plugin/updater/
- Shell plugin: https://v2.tauri.app/plugin/shell/
- Filesystem plugin: https://v2.tauri.app/plugin/file-system/
- HTTP client plugin: https://v2.tauri.app/plugin/http-client/
- Localhost plugin: https://v2.tauri.app/plugin/localhost/
- Stronghold plugin: https://v2.tauri.app/plugin/stronghold/
- Tauri source and issues: https://github.com/tauri-apps/tauri

Check plugin permission pages for the exact installed plugin version. Do not transfer Tauri 1 allowlist assumptions into Tauri 2 capability designs or vice versa.

## Rust and Cargo

- Rust standard library: https://doc.rust-lang.org/std/
- Rust Reference: https://doc.rust-lang.org/reference/
- Rust Edition Guide: https://doc.rust-lang.org/edition-guide/
- Cargo Book: https://doc.rust-lang.org/cargo/
- Cargo profiles: https://doc.rust-lang.org/cargo/reference/profiles.html
- Cargo features: https://doc.rust-lang.org/cargo/reference/features.html
- Clippy usage: https://doc.rust-lang.org/clippy/usage.html
- Clippy lint list: https://rust-lang.github.io/rust-clippy/master/
- Rustonomicon for unsafe Rust: https://doc.rust-lang.org/nomicon/
- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- Rust compiler and library source: https://github.com/rust-lang/rust

Use docs.rs for crate APIs and select the exact crate version from the lockfile. Read crate source when behavior, safety, cancellation, or platform support is not explicit in documentation.

## Async and Tokio

- Async Book: https://rust-lang.github.io/async-book/
- Tokio documentation: https://docs.rs/tokio/
- `spawn_blocking`: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
- Tokio synchronization: https://docs.rs/tokio/latest/tokio/sync/
- Tokio graceful shutdown topic: https://tokio.rs/tokio/topics/shutdown

Confirm which runtime Tauri and the repository use. Do not introduce a second async runtime without a measured and documented reason.

## Security and dependency policy

- RustSec Advisory Database: https://rustsec.org/
- cargo-audit: https://github.com/rustsec/rustsec/tree/main/cargo-audit
- cargo-deny: https://embarkstudios.github.io/cargo-deny/
- crates.io package metadata: https://crates.io/

Advisory tools find known issues. Review maintainership, source, unsafe code, features, build scripts, and transitive dependencies separately.

## Source selection rules

1. Prefer project source and configuration for current behavior.
2. Prefer official documentation for supported APIs and security model.
3. Prefer exact-version docs and crate source over latest-version examples.
4. Prefer upstream issue trackers for confirmed limitations.
5. Treat blog posts and generated examples as secondary.
6. Cite or link the source for a consequential version-sensitive recommendation.
7. State uncertainty when behavior could not be verified.
