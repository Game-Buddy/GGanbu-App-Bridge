# Architecture Decision Guide

## Contents

1. Decision sequence
2. Recommended boundaries
3. Tauri command design
4. State ownership and concurrency
5. Error design
6. Data and persistence
7. Cross-platform design
8. Dependency decisions
9. Architecture smells

## 1. Decision sequence

Use this sequence before choosing modules, crates, traits, or concurrency:

1. Define the user-visible behavior and failure behavior.
2. Identify trusted and untrusted inputs.
3. Identify state owners and lifecycle boundaries.
4. Identify operations that touch the OS, network, filesystem, process table, credentials, updater, or external devices.
5. Separate business policy from Tauri and platform mechanics.
6. Decide the smallest public contracts between components.
7. Decide how each contract is tested without a WebView or real operating-system side effect.
8. Decide observability, cancellation, shutdown, migration, and compatibility behavior.
9. Add abstractions only where they protect one of those decisions.

Prefer a direct design over a pattern name. Explain the concrete dependency direction and invariants.

## 2. Recommended boundaries

Use these conceptual boundaries when the application is large enough to benefit. Do not require every project to have every directory.

- **Domain:** Pure rules, value objects, state machines, and invariants. Do not depend on Tauri, UI frameworks, storage, HTTP, or platform APIs.
- **Application:** Use cases, orchestration, authorization policy, transactions, and ports needed from infrastructure.
- **Adapters:** Tauri commands, plugins, databases, filesystems, keychains, local servers, HTTP clients, input injection, sidecars, and platform implementations.
- **Composition root:** Construct concrete services, register commands and plugins, initialize state, and define shutdown order.
- **Frontend interface:** Typed request and response models, a centralized IPC client, view models, and UI state.

A reasonable Rust layout for a non-trivial application is:

```text
src-tauri/src/
  lib.rs                 # composition root and Tauri builder
  interface/
    commands/            # thin #[tauri::command] adapters
    dto/                 # serialized boundary types
  application/           # use cases and policy
  domain/                # pure types and rules
  infrastructure/        # persistence, network, OS integrations
  platform/              # narrowly isolated cfg-specific adapters
```

Adapt names to the repository. Do not move stable code merely to match this example.

Enforce dependency direction toward domain and application code. Infrastructure may implement application-defined traits; domain code must not import infrastructure types.

Avoid one generic `services` or `utils` module that becomes an unowned dependency hub. Name modules after capabilities or concepts.

## 3. Tauri command design

Treat every command as a public privileged endpoint even when only bundled frontend code calls it.

Use a command flow like:

```text
frontend typed client
  -> serialized request DTO
  -> Tauri command adapter
  -> validation and authorization
  -> application use case
  -> domain and infrastructure
  -> typed result
  -> sanitized response DTO
```

Require commands to:

- Accept narrow request DTOs instead of maps or arbitrary JSON.
- Reject unknown or unsupported operations explicitly.
- Validate format, length, range, path, URL, identifier, and state.
- Perform authorization in Rust, not only in disabled UI controls.
- Enforce timeouts and resource limits for expensive work.
- Return stable error codes or variants plus a user-safe message.
- Avoid returning filesystem paths, tokens, stack traces, SQL errors, or internal secrets unless the contract requires and protects them.
- Delegate business logic to a testable application service.

Prefer multiple narrow commands over a generic `execute(action, payload)` command. Never expose arbitrary shell commands, process arguments, SQL, file paths, URLs, or keyboard sequences without a strict allowlist and policy layer.

Centralize frontend access:

```text
src/lib/ipc/
  client.ts
  contracts.ts
  errors.ts
```

Use generated bindings only when the repository already supports them reliably. Otherwise keep matching Rust and TypeScript contracts together and add serialization contract tests.

Version persisted and externally consumed data. For internal IPC, prefer additive changes and explicit defaults; do not silently reinterpret an existing field.

## 4. State ownership and concurrency

Assign one clear owner to every mutable resource.

Prefer:

- Immutable configuration shared by value or `Arc`.
- A service object that owns each database pool, client, device connection, local server, or background supervisor.
- Message passing for long-running workers with serialized state transitions.
- Short lock scopes around only the data that must be shared.
- Snapshot or copy-on-write reads when reads dominate and state is modest.

Avoid:

- Global mutable statics.
- One application-wide mutex guarding unrelated state.
- A mutex held while awaiting I/O or invoking frontend code.
- Fire-and-forget tasks without a supervisor, cancellation signal, and error path.
- State duplicated independently in Rust and frontend without an ownership rule.

For each background task, define:

- Who starts it
- How duplicate starts are prevented
- How it receives work
- Maximum queued work and backpressure behavior
- How cancellation and application exit work
- How errors are surfaced
- Whether restart is safe and how state is recovered

Prefer idempotent operations when commands may be retried. Use request identifiers only when duplicate effects are possible and material.

## 5. Error design

Separate errors by boundary:

- **Domain errors:** violated business rule or invalid state transition
- **Application errors:** authorization, conflict, unavailable dependency, cancellation, timeout
- **Infrastructure errors:** OS, I/O, database, HTTP, plugin, serialization, platform failure
- **Interface errors:** stable code and sanitized message returned over IPC

Preserve diagnostic causes internally while exposing only necessary detail. Log correlation identifiers rather than sensitive payloads.

A useful command error contract contains:

```text
code: stable machine-readable identifier
message: safe user-facing explanation
retryable: optional behavior hint
correlation_id: optional support identifier
field_errors: optional structured validation failures
```

Do not turn every error into a string too early. Do not make frontend logic parse English text.

Use panics only for proven startup invariants that make continued execution impossible. Prefer returning startup errors with context when operators can fix the cause.

## 6. Data and persistence

Choose persistence from requirements, not convenience:

- Use an in-memory model for ephemeral state that can be reconstructed.
- Use an atomic file for small single-document configuration with simple migration needs.
- Use a transactional database when there are concurrent writes, relationships, queries, or durable workflows.
- Use the operating-system credential store or a hardened secret store for credentials.

Define:

- Schema version and migration direction
- Atomicity and crash behavior
- File permissions and backup behavior
- Encryption requirements and key ownership
- Data retention and deletion
- Corruption detection and recovery
- Compatibility with downgrade and rollback

Use atomic replace for critical file updates. Decide whether durability requires flushing file and directory metadata; do not assume a successful write call is power-loss durable.

## 7. Cross-platform design

Keep platform variance at the edges.

- Define a small trait or service contract for behavior that genuinely differs.
- Put `cfg` branches in platform modules or the composition root, not throughout domain code.
- Use platform-specific capability files when permissions differ.
- Normalize semantic behavior, not necessarily implementation details.
- Return an explicit unsupported error for unavailable features.
- Test each supported target in native CI or a documented release validation process.

Account for:

- Path and filename rules
- Case sensitivity
- Process and signal behavior
- Window and tray lifecycle
- Installer and update behavior
- Code signing and notarization
- Permission prompts and sandboxing
- Keyboard layouts and input APIs
- WebView engine differences
- Filesystem and keychain semantics

Do not claim cross-platform support based only on successful compilation on one operating system.

## 8. Dependency decisions

Before adding a dependency, record:

1. The capability it provides
2. Why existing dependencies or a small local implementation are insufficient
3. Maintainer and release health
4. License and source policy
5. MSRV and target support
6. Default and optional features
7. Transitive dependency and binary-size impact
8. Unsafe code and native build requirements
9. Security advisory history
10. Exit or replacement strategy

Prefer official Tauri plugins for platform capabilities when their permission model and maintenance fit the requirement. Do not adopt a plugin merely to avoid writing a few lines of safe Rust.

Use dependency features narrowly. Avoid duplicate runtime, TLS, HTTP, serialization, or async stacks without a measured reason.

## 9. Architecture smells

Investigate these signals:

- Commands contain business rules, database queries, and platform calls together.
- The frontend decides whether a privileged operation is allowed.
- A generic command accepts arbitrary action names or payloads.
- Every module can reach global application state.
- A single mutex protects the entire application.
- Background tasks are spawned without ownership or shutdown.
- Domain types are serialized directly and become accidental public contracts.
- Platform checks appear throughout otherwise portable logic.
- An abstraction has one implementation, no testing benefit, and no protected boundary.
- Error strings cross several layers without stable categories.
- Performance work begins with cloning or allocation cleanup before profiling.
- Documentation describes folders but not dependencies, trust boundaries, or invariants.
