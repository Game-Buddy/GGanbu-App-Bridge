# Tauri Engineering Guide

## Contents

1. Version and process model
2. Composition root
3. Commands, events, and streaming
4. Managed state and services
5. Windows, WebViews, and privilege
6. Plugins, capabilities, and custom commands
7. Local services and sidecars
8. Frontend integration
9. Updater, packaging, and release
10. Tauri review checklist

## 1. Version and process model

Identify the installed Tauri major version before applying patterns. Tauri 1 allowlists and Tauri 2 capabilities are not interchangeable.

Treat the application as at least two security and performance domains:

- Rust core with operating-system privileges
- WebView frontend with web-platform risks and IPC access

Additional WebViews, local servers, sidecars, plugins, and remote content create more boundaries. Draw them before adding privileges.

Use official documentation matching the installed Tauri and plugin versions. Inspect generated schemas in `src-tauri/gen/schemas` when available because they reflect installed permissions.

## 2. Composition root

Keep Tauri builder setup in one obvious composition root.

The composition root should:

- Construct configuration and services.
- Register only required plugins.
- Manage shared state.
- Register commands.
- Configure windows, tray, deep links, and lifecycle hooks.
- Start supervised background services.
- Define shutdown behavior.
- Map startup failures with context.

Keep domain and application modules free from `AppHandle`, `Window`, `WebviewWindow`, plugin handles, and Tauri-specific result types unless the behavior is inherently Tauri-facing.

Avoid a large `setup` closure containing migrations, network calls, blocking initialization, worker loops, and UI manipulation. Delegate to named services and keep startup measurable.

Use library-style `run()` composition when it improves unit testing and platform entrypoint reuse. Follow current Tauri mobile entrypoint requirements for projects targeting mobile.

## 3. Commands, events, and streaming

Choose the communication primitive by semantics:

- **Command:** Request-response operation initiated by the frontend.
- **Event:** Notification or broadcast where receivers may be absent and no direct result is required.
- **Channel or supported stream mechanism:** Ordered or sustained data flow with defined ownership and backpressure behavior.
- **Managed state query:** Still expose through a command; do not let frontend directly access Rust state.

Commands:

- Use narrow DTOs and typed responses.
- Keep command functions thin.
- Validate and authorize in Rust.
- Make retries and duplicate effects explicit.
- Add timeouts and cancellation for long work.

Events:

- Do not use an event name as authentication.
- Avoid placing secrets or sensitive raw data in global events.
- Scope delivery to the intended window when possible.
- Version payloads when multiple frontend versions can exist.
- Define behavior for missed or lagging events.

High-frequency data:

- Batch or coalesce.
- Limit payload size and rate.
- Prefer snapshots plus sequence numbers when recovery matters.
- Remove listeners when windows or components close.
- Measure serialization and render cost.

Do not build a second ad hoc message bus on top of stringly typed events when commands and typed channels satisfy the requirement.

## 4. Managed state and services

Store service handles in Tauri-managed state, not arbitrary mutable data shared everywhere.

A managed service should own:

- Its external resource or worker
- Internal synchronization
- Cancellation and shutdown
- Metrics or diagnostic state
- A narrow application-facing API

Prefer `State<'_, Service>` or state containing an `Arc<Service>` according to lifecycle and API needs. Do not wrap every service in `Arc<Mutex<_>>` by default.

Never hold a managed-state lock while emitting an event, awaiting network I/O, waiting for a child process, or calling frontend-controlled behavior.

If state initialization can fail, fail startup with context or model a deliberate unavailable state. Do not hide partial initialization behind panicking access.

## 5. Windows, WebViews, and privilege

Use stable window and WebView labels as security identifiers. Titles are presentation, not identity.

- Give each privilege class its own label and capability.
- Avoid assigning one window to multiple broad capabilities.
- Restrict who can create new windows or navigate existing windows.
- Prevent privileged windows from loading untrusted remote content.
- Define behavior when a window reloads, crashes, closes, or is recreated.
- Remove window-specific listeners and session state on close.
- Validate deep links and external navigation before acting.

For tray applications, define close versus hide versus quit behavior. Ensure background workers and local services do not outlive the intended app lifecycle.

Do not put secrets in window URLs, query parameters, titles, or frontend storage.

## 6. Plugins, capabilities, and custom commands

Add a plugin only when its API and permission model fit the requirement. Registering a plugin does not justify granting its broad default permission set.

For Tauri 2:

- Inspect plugin permissions and command scopes.
- Use per-window, per-WebView, and per-platform capabilities.
- Explicitly enumerate enabled capability identifiers.
- Use generated schemas for configuration validation.
- Review scopes for filesystem paths, shell arguments, URLs, and other resources.
- Account for capability merging.

Registered application commands need separate attention. Do not assume plugin capability configuration automatically restricts every custom `#[tauri::command]`. Verify current Tauri behavior and, where applicable, declare application commands through the build manifest and define explicit command permissions and capabilities. Inspect `build.rs`, generated permission schemas, and the command registration list together.

Keep `invoke_handler` registration auditable. Avoid generating or concatenating a large command list from hidden macro layers without documentation.

Use the frontend form of a plugin only for operations the frontend should directly invoke. For highly privileged workflows, call the plugin from Rust behind application policy and expose a narrower command.

## 7. Local services and sidecars

Use a local HTTP or WebSocket service only when a browser, external process, or streaming requirement cannot use direct Tauri IPC.

Local service:

- Bind to loopback.
- Authenticate sessions.
- Validate origin, host, protocol, messages, and limits.
- Supervise listener lifecycle and port ownership.
- Keep actions finite and policy-controlled.
- Revoke sessions on disconnect, lock, or re-pairing.
- Expose health without leaking secrets.

Sidecar:

- Package and address the exact sidecar binary for every target.
- Restrict arguments and environment.
- Verify startup handshake and version compatibility.
- Bound output and restart behavior.
- Terminate child processes on app shutdown and crash recovery where feasible.
- Include sidecars in signing, updater, license, and supply-chain review.

Do not use a sidecar to bypass Rust ownership or permission design. It is another privileged process and attack surface.

## 8. Frontend integration

Create one typed frontend boundary module.

It should:

- Export request and response types or generated bindings.
- Wrap command names and plugin calls.
- Normalize interface errors.
- Apply frontend cancellation and stale-response handling.
- Subscribe and unsubscribe events centrally.
- Avoid exposing raw privileged primitives to arbitrary components.

Frontend validation improves usability but is not a security control. Repeat enforcement in Rust.

Keep secret and authority state in Rust. Frontend state may represent display and optimistic interaction, but Rust remains authoritative for privileged transitions.

When using remote frontend content, treat the origin as a separate application with a much larger threat model. Default to bundled content.

## 9. Updater, packaging, and release

Design updater and packaging during architecture, not after feature completion.

- Define application identifier and signing identities early.
- Protect updater signing keys and platform signing credentials.
- Generate and verify signed update artifacts.
- Separate stable, beta, and internal channels.
- Map every target and architecture to the correct artifact.
- Define migration compatibility and rollback behavior.
- Test update from supported previous versions, not only clean install.
- Preserve user data and settings through install, update, and uninstall according to product policy.
- Keep release CI least privilege and protected from untrusted pull-request code.

Review platform-specific requirements for Windows installers, macOS signing and notarization, Linux packages, and any mobile targets. A successful local `tauri build` is not complete release validation.

Measure the packaged artifact, startup, updater size, and sidecar contribution when performance or distribution size matters.

## 10. Tauri review checklist

Confirm:

- Installed Tauri and plugin versions are identified.
- Composition root and shutdown path are understandable.
- Commands are narrow, validated, authorized, and tested.
- Events do not leak secrets or become an authorization mechanism.
- Streaming is bounded and has lag behavior.
- Managed state has clear ownership and no accidental lock across await.
- Window labels align with privilege boundaries.
- Capability files, custom commands, plugin permissions, and scopes are all reviewed.
- CSP and remote navigation are restricted.
- Local services and sidecars are authenticated, bounded, and supervised.
- Frontend IPC is centralized and typed.
- Updater artifacts and installers are signed and tested.
- Native behavior is validated on every supported platform.
