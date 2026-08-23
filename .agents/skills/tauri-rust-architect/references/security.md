# Tauri and Rust Security Guide

## Contents

1. Threat-model baseline
2. WebView and IPC boundary
3. Capabilities, permissions, and scopes
4. CSP and remote content
5. Local HTTP and WebSocket bridges
6. Filesystem and path handling
7. Shell, process, and sidecar execution
8. Network and URL handling
9. Secrets and local storage
10. Updates, signing, and release integrity
11. Logging, diagnostics, and privacy
12. Rust unsafe code and memory safety
13. Supply chain
14. Security completion checklist

## 1. Threat-model baseline

Assume the following unless the application proves a stronger boundary:

- Frontend code can be compromised through XSS, an unsafe dependency, remote content, or a WebView issue.
- Every IPC argument can be attacker-controlled.
- Another local process can attempt to connect to localhost services and read user-accessible files.
- Network responses, update metadata, imported files, deep links, clipboard data, command-line arguments, and device data are untrusted.
- A compromised operating-system account or kernel is outside the protection boundary, but the application must still minimize secrets and blast radius.
- Third-party Rust crates, npm packages, plugins, build scripts, and CI actions are part of the supply chain.

For sensitive work, document:

- Assets: credentials, local files, user identity, system control, keyboard input, update channel, device access, business data
- Actors: normal user, malicious website, local unprivileged process, network attacker, compromised dependency, malicious update publisher
- Entry points: commands, events, deep links, files, URLs, local ports, sidecars, updater, environment, CLI
- Trust boundaries: WebView to Rust, Rust to OS, app to local service, app to network, build to release
- Abuse cases and required controls
- Residual risks and assumptions

Treat security as behavior to test, not only configuration to inspect.

## 2. WebView and IPC boundary

Treat Tauri commands as privileged API endpoints.

For every command:

- Use a dedicated request type.
- Set practical maximum lengths and collection sizes.
- Parse identifiers, paths, URLs, and enums into validated types.
- Reject unknown operations and impossible state transitions.
- Re-check authorization and connection ownership in Rust.
- Apply timeout, cancellation, concurrency, and rate limits where abuse can consume resources.
- Return minimum necessary data.
- Sanitize user-visible errors.
- Add negative tests for malformed, oversized, unauthorized, stale, repeated, and out-of-order requests.

Never rely on:

- A hidden or disabled frontend button
- TypeScript types alone
- CORS alone
- A localhost address alone
- An unguessable command name
- The assumption that only bundled code can call a command

Do not expose generic primitives such as arbitrary command execution, unrestricted file reads, raw SQL, arbitrary URL fetch, or user-defined keyboard macros. Expose a finite application operation with policy.

## 3. Capabilities, permissions, and scopes

Use the Tauri 2 capability system as a least-privilege boundary, not as a convenience list.

- Explicitly enumerate capability identifiers used by the application build.
- Inspect every file under `src-tauri/capabilities`.
- Scope by exact window or WebView label.
- Use platform-specific capabilities when behavior differs.
- Grant individual plugin operations rather than a broad default set when feasible.
- Use command scopes to constrain paths, URLs, executables, and other resources.
- Account for permissions merged when a window or WebView belongs to multiple capabilities.
- Restrict window creation APIs because a newly created privileged window can alter the boundary.
- Avoid remote capability access. When unavoidable, restrict exact origins and review platform-specific iframe behavior.
- Add a test or release check that detects unexpected capability expansion.
- Review custom application commands separately from plugin commands. For Tauri 2 versions where registered commands are otherwise available broadly, declare the application command list through the build manifest and attach explicit permissions and capabilities as appropriate. Verify exact behavior against the installed version.

Flag for review:

- `windows: ["*"]` or `webviews: ["*"]`
- `permissions: ["*"]` or plugin-wide broad permissions
- Home-directory or filesystem-recursive scopes
- Arbitrary shell argument scopes
- Broad HTTP host patterns
- Remote URLs in capabilities
- Capability files that are present but not understood by the team

## 4. CSP and remote content

Configure a restrictive CSP for production.

- Default to bundled local frontend assets.
- Avoid remote scripts and executable content.
- Use exact trusted origins for necessary network, image, or font sources.
- Avoid `unsafe-eval`; justify and constrain any `unsafe-inline` use.
- Keep development CSP exceptions out of production configuration.
- Review custom protocols, asset scopes, iframe sources, navigation, and window opening.
- Disable global Tauri API exposure unless a documented integration requires it.
- Do not disable Tauri CSP asset processing without a reviewed reason.

A CSP reduces exploit impact but does not authorize privileged operations. Rust-side validation remains mandatory.

## 5. Local HTTP and WebSocket bridges

A loopback service is reachable by other local processes and potentially by browser pages. Design it as an authenticated service.

Default controls:

- Bind only to loopback addresses, never all interfaces.
- Use an ephemeral port or explicitly protect a fixed port against conflicts and impersonation.
- Generate a high-entropy session secret in Rust.
- Use a low-entropy pairing code only to bootstrap a high-entropy session; expire it quickly and rate-limit attempts.
- Never place long-lived secrets in URLs, logs, process arguments, analytics, or browser history.
- Authenticate the HTTP request or WebSocket handshake before accepting privileged messages.
- Validate `Origin` against an exact allowlist when a browser origin is known. Treat a missing or forgeable origin according to the client threat model.
- Validate `Host`, method, content type, protocol version, and message schema.
- Set strict request and frame size limits.
- Use bounded connections and queues, idle timeouts, heartbeat or liveness behavior, and explicit shutdown.
- Allow only named application actions. Never forward arbitrary key sequences, shell commands, paths, or JSON to an executor.
- Add replay protection or idempotency where duplicate actions cause harm.
- Define single-client ownership explicitly if only one client is allowed.
- Rotate or revoke the session when the client disconnects, the app locks, or a new pairing occurs.

CORS controls browser response access; it is not authentication and does not stop all request types. DNS rebinding, malicious local processes, stale sessions, and port hijacking need separate consideration.

For WebSockets:

- Authenticate during the upgrade.
- Check the origin.
- Use a versioned message envelope and finite message types.
- Reject messages before deserializing large nested content when possible.
- Bound pending outbound messages and choose a drop, coalesce, or disconnect policy.
- Do not let slow clients retain unbounded state.

## 6. Filesystem and path handling

Prefer application-owned directories and scoped Tauri filesystem APIs.

For user-supplied paths:

- Decide whether the contract accepts a logical identifier, relative path, or absolute path. Prefer logical identifiers.
- Reject path traversal and unexpected prefixes.
- Constrain operations to an allowed root.
- Account for symlinks, junctions, aliases, case differences, UNC paths, device paths, and platform-specific prefixes.
- Avoid check-then-use logic that can be changed between validation and opening.
- Set restrictive permissions for sensitive files.
- Use atomic writes for configuration and durable state.
- Limit file size and streaming memory usage.
- Reject unexpected file types based on parsed content when security depends on type; extensions alone are insufficient.

Canonicalization can help compare paths but does not by itself prevent time-of-check/time-of-use races or symlink replacement.

## 7. Shell, process, and sidecar execution

Prefer direct library or OS APIs. When a process is necessary:

- Use a fixed executable or packaged sidecar.
- Pass arguments as an argument array, never by concatenating a shell command string.
- Avoid invoking a shell.
- Use an allowlist of operations and validate each argument independently.
- Set a controlled working directory and environment.
- Remove secrets from environment and arguments where they may be inspected.
- Bound stdout and stderr collection.
- Set timeout, cancellation, child-process cleanup, and exit-code handling.
- Validate sidecar integrity and release packaging.
- Scope Tauri shell permissions to exact executables and arguments.

Never expose a frontend command that accepts an executable plus arbitrary arguments.

## 8. Network and URL handling

Parse URLs with a real parser and validate after parsing.

- Allow exact schemes, hosts, and ports.
- Reject username, password, fragment, and alternate schemes unless required.
- Constrain redirects and re-validate every redirect target.
- Limit response size, decompression ratio, timeout, and retries.
- Avoid sending credentials to redirected or user-controlled hosts.
- Use TLS and normal certificate validation.
- Protect against server-side request forgery when the destination is influenced by input.
- Consider DNS rebinding and resolved private addresses for restricted network clients.
- Keep update, telemetry, and business endpoints separate when their trust differs.

Do not place credentials in query strings. Redact authorization headers and sensitive bodies from logs.

## 9. Secrets and local storage

Do not compile secrets into frontend or Rust binaries. Public API identifiers are not secrets.

- Store persistent credentials in an operating-system credential facility or an appropriately hardened secret store.
- Keep secret lifetime in memory as short as practical.
- Avoid unnecessary clones and string formatting of secrets.
- Redact `Debug` and serialization output.
- Zeroization may reduce exposure for high-value secrets, but verify that the full data path supports it.
- Separate authentication tokens from low-sensitivity preferences.
- Define key rotation, logout, revocation, backup, and device transfer behavior.
- Encrypting a file is useful only when key ownership and access control are stronger than the file itself.

## 10. Updates, signing, and release integrity

Treat the updater as a remote code execution channel controlled by the publisher.

- Sign updater artifacts and configure signature verification.
- Protect the private signing key outside the repository and normal build logs.
- Back up the key securely and define rotation or compromise response.
- Publish update metadata over TLS.
- Validate channel, target, architecture, version, and signature.
- Keep stable and pre-release channels separate.
- Define rollback behavior and data-schema compatibility.
- Do not disable version checks or change the updater public key dynamically without an explicit design.
- Sign and notarize platform installers as required.
- Restrict who and what CI workflow can publish a release.
- Pin or review release actions and protect environment secrets.

## 11. Logging, diagnostics, and privacy

Use structured, leveled logs with a documented retention policy.

Never log:

- Passwords, tokens, cookies, authorization headers, private keys, pairing secrets
- Full sensitive payloads or user documents
- Unredacted filesystem paths when they reveal personal data unnecessarily
- Keystrokes or injected actions beyond what support and consent require

Use correlation IDs, event categories, and sanitized error context. Keep diagnostic logging opt-in when it may capture personal or security-sensitive information.

Prevent log forging by using structured fields rather than concatenated untrusted text. Bound log file size and rotation.

## 12. Rust unsafe code and memory safety

Default to safe Rust.

When unsafe code is unavoidable:

- State why safe alternatives do not satisfy the requirement.
- Minimize the unsafe block.
- Document every caller and callee invariant in a `SAFETY` comment.
- Wrap unsafe behavior in the smallest safe API that can enforce the invariant.
- Audit pointer provenance, aliasing, initialization, lifetimes, thread safety, FFI ownership, unwind behavior, and platform ABI.
- Test boundary values and failure paths.
- Use Miri, sanitizers, fuzzing, or platform tools where applicable.
- Re-audit when dependency versions, compiler versions, or target platforms change.

Do not use unsafe code as a performance optimization without benchmark evidence and a demonstrated need.

## 13. Supply chain

For Rust and frontend dependencies:

- Review lockfile changes.
- Run advisory checks.
- Use `cargo deny` or equivalent policy when configured for advisories, licenses, sources, and duplicate versions.
- Minimize build scripts, native code, and dependencies fetched from mutable branches.
- Inspect feature flags and disable unnecessary defaults.
- Review npm lifecycle scripts and package provenance.
- Pin CI actions according to organizational policy.
- Protect release credentials with least-privilege environments.
- Generate or retain an SBOM when the product or organization requires it.

An advisory scanner detects known issues; it does not establish that a dependency is trustworthy.

## 14. Security completion checklist

Before declaring sensitive work complete, confirm:

- Trust boundaries and assets are documented at the appropriate depth.
- Command inputs are validated and authorized in Rust.
- Capabilities and plugin scopes are minimal.
- CSP and remote-content policy are reviewed.
- Local services require authentication and bind only as intended.
- Paths, URLs, process arguments, and payload sizes are constrained.
- Secrets are not exposed to frontend, logs, repository, or errors.
- Failure, timeout, retry, cancellation, and shutdown behavior are safe.
- Update and release integrity is preserved.
- Dependencies and lockfile changes are reviewed.
- Negative and abuse-case tests exist.
- Residual risks are explicit.
