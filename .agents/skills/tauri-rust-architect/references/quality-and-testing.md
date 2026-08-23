# Quality and Testing Guide

## Contents

1. Validation strategy
2. Test selection matrix
3. Rust checks
4. Frontend and IPC checks
5. Security testing
6. Concurrency and lifecycle testing
7. Cross-platform validation
8. Performance validation
9. CI policy
10. Evidence standard

## 1. Validation strategy

Validate the highest-risk behavior at the lowest practical layer, then add boundary and end-to-end coverage where integration can fail.

Use this sequence:

1. Reproduce the bug or define an executable acceptance test.
2. Test pure domain and application behavior without Tauri.
3. Test adapters with fakes, temporary resources, or controlled integration services.
4. Test serialization and command boundary behavior.
5. Test frontend behavior and typed IPC handling.
6. Test a packaged or near-packaged application for critical user journeys.
7. Validate every supported platform through native CI or documented manual release checks.

Do not require a full end-to-end test for behavior a fast unit test proves. Do not rely only on unit tests for permissions, packaging, updater, installer, or WebView integration.

## 2. Test selection matrix

| Risk                        | Minimum useful evidence                                                                   |
| --------------------------- | ----------------------------------------------------------------------------------------- |
| Pure business rule          | Unit tests including invalid transitions and boundaries                                   |
| Tauri command               | Request validation, authorization, result mapping, and serialization tests                |
| Filesystem                  | Temporary-directory tests, traversal and symlink cases, permission failures               |
| Local HTTP/WebSocket bridge | Authentication, origin, replay, limits, disconnect, shutdown, and concurrent client tests |
| Process or sidecar          | Exact argument construction, timeout, cancellation, exit code, output limit, cleanup      |
| Persistent schema           | Forward migration, interrupted migration, rollback policy, old-data fixture               |
| Background worker           | Queue bounds, duplicate start, cancellation, failure propagation, shutdown                |
| Concurrency                 | Deterministic state-machine tests plus stress or model checks where justified             |
| Security-sensitive parser   | Property tests or fuzzing when input complexity warrants it                               |
| Performance claim           | Repeatable release benchmark and correctness comparison                                   |
| Cross-platform adapter      | Native compile and behavior check on each supported target                                |
| Updater or release          | Signed artifact, metadata, channel, target, rollback, and CI permission checks            |

## 3. Rust checks

Discover repository-native commands first. Typical checks are:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Adapt feature combinations when `--all-features` enables mutually exclusive or unsupported configurations. Test the production feature set explicitly.

Use targeted tools when the risk justifies them:

- `cargo audit` for known RustSec advisories
- `cargo deny check` for advisories, licenses, sources, bans, and duplicates when configured
- Miri for unsafe code and undefined-behavior-sensitive logic
- Sanitizers for native memory, thread, or address issues where supported
- Fuzzing for parsers, protocol messages, imported data, or unsafe boundaries
- Property testing for state machines and validation invariants
- Loom or a deterministic concurrency model for subtle synchronization algorithms
- Coverage reports to find untested critical paths, not as a quality score by itself

Do not install tools, update the lockfile, or modify toolchain configuration silently. Report a skipped check and the reason.

Test errors, timeouts, cancellation, and resource exhaustion. A happy-path-only test suite is insufficient for systems code.

## 4. Frontend and IPC checks

Run the repository package-manager commands for:

- Formatting
- Linting
- Type checking
- Unit and component tests
- Production frontend build
- Dependency or lockfile policy

Test IPC as a contract:

- Request field names and serialized enum representations
- Optional and default fields
- Unknown and malformed fields according to compatibility policy
- Error codes and retryability
- Numeric range and precision behavior
- Date, time, path, URL, and binary representations
- Backward compatibility for persisted or independently versioned clients

Centralize a mock IPC transport so frontend tests exercise the same typed client used in production. Avoid mocking every UI component at the raw Tauri API level.

For critical flows, use Tauri-supported WebDriver or an equivalent packaged-app test where feasible. Do not claim browser-only tests prove Tauri permissions or native integration.

## 5. Security testing

Write negative tests for every privileged boundary.

Include applicable cases:

- Missing, invalid, expired, replayed, or revoked authentication
- Wrong client or connection ownership
- Unauthorized command or state transition
- Oversized strings, collections, files, frames, and nested payloads
- Path traversal, alternate separators, symlinks, junctions, and case variants
- Unsupported URL scheme, host, port, redirect, userinfo, and private-address target
- Shell metacharacters and argument-boundary attacks
- Invalid origin, host, method, content type, and protocol version
- Duplicate, out-of-order, and concurrent requests
- Slow client, queue saturation, retry storm, and timeout
- Secret redaction in errors and logs
- Capability or permission regression

When testing a local browser bridge, include a page from an unauthorized origin and a direct local client that does not behave like a browser. CORS success or failure is not sufficient authentication evidence.

Verify release configuration separately from development configuration. Security often differs through dev URLs, CSP exceptions, debug endpoints, signing, and updater settings.

## 6. Concurrency and lifecycle testing

Make lifecycle behavior testable through explicit supervisors, cancellation tokens, or service handles.

Test:

- Duplicate initialization
- Concurrent requests for the same and different resources
- Cancellation before start, during work, and after external side effects
- Application exit with queued and active work
- Worker failure and restart policy
- Lost frontend or local client connection
- Lock poisoning or service failure where applicable
- Backpressure and item-drop policy
- Event ordering and stale snapshots
- Child process cleanup

Avoid sleep-based tests as the primary synchronization mechanism. Use barriers, channels, injected clocks, and deterministic fakes. If timing is unavoidable, use generous bounds and isolate the test from loaded shared CI workers.

## 7. Cross-platform validation

Maintain a target matrix that names:

- Operating system and supported versions
- Architecture
- Rust target
- WebView dependency
- Installer format
- Signing and notarization requirement
- Platform-only features and permissions

At minimum, compile and test core Rust logic on all supported targets. Run native integration and packaging checks for platform adapters. Validate installer and updater behavior before release.

Do not infer macOS or Windows behavior from Linux CI. Do not infer ARM behavior from x86 compilation alone when native code, sidecars, or performance matters.

Record platform checks that require hardware or credentials and make them part of the release checklist.

## 8. Performance validation

For a performance change:

1. Freeze a representative workload and dataset.
2. Measure the baseline in a production-equivalent release build.
3. Record environment and sample distribution.
4. Apply the change.
5. Re-run identical measurements.
6. Run correctness, security, and soak checks.
7. Check related metrics such as memory, CPU, startup, binary size, and failure rate.

Use microbenchmarks for isolated algorithms and end-to-end traces for user-visible latency. Neither replaces the other.

Guard stable hot paths with benchmarks only when CI variance can be managed. Otherwise retain a reproducible benchmark command and compare during performance work.

## 9. CI policy

Prefer separate, understandable CI jobs:

- Format and static analysis
- Rust unit and integration tests
- Frontend lint, type check, and tests
- Security and dependency policy
- Platform build matrix
- Packaged-app or smoke tests
- Release signing and publication

Use least-privilege workflow permissions. Keep pull-request CI unable to publish releases or access production secrets. Pin or review third-party actions according to organization policy.

Fail on new warnings when the repository is already warning-clean. Do not hide warnings globally to make a change pass.

Cache dependencies and build output only when cache keys include relevant lockfiles, toolchains, target, and feature inputs. Treat caches as untrusted acceleration, not release provenance.

## 10. Evidence standard

Report validation as facts:

```text
PASS: cargo test --workspace --all-features (142 tests)
PASS: pnpm typecheck
PASS: local bridge rejects missing token and unauthorized Origin
NOT RUN: macOS packaging; no macOS runner available
FAILED: cargo audit; one existing advisory in dependency X
```

Never say tests pass when only code inspection was performed. Never omit a failing check. Distinguish existing failures from failures introduced by the change with evidence.
