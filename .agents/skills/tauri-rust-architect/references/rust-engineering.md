# Rust Engineering Guide

## Contents

1. Type and API design
2. Ownership and borrowing
3. Error and panic policy
4. Async and concurrency
5. Serialization and boundary types
6. Traits, generics, and abstraction
7. Unsafe Rust and FFI
8. Performance and allocation
9. Cargo features and workspace design
10. Readability and review checklist

## 1. Type and API design

Use the type system to make valid behavior easy and invalid behavior difficult.

- Introduce newtypes for identifiers, validated paths, URLs, secrets, sequence numbers, and units when primitive confusion is plausible.
- Parse untrusted primitives into validated domain types at the boundary.
- Model finite state with enums rather than booleans whose combinations can be invalid.
- Keep fields private when construction must enforce invariants.
- Provide constructors or conversion implementations that return a typed error.
- Keep public APIs narrow. Prefer `pub(crate)` or private implementation details until a real consumer needs them.
- Mark important return values `#[must_use]` when silently discarding them is likely a bug.
- Use exhaustive matching inside the crate. Use non-exhaustive public types only when forward compatibility outweighs consumer ergonomics.
- Avoid exposing dependency types in public APIs unless the dependency is deliberately part of the contract.

Prefer a state transition method that owns its invariant:

```rust
impl Connection {
    pub fn authenticate(self, proof: Proof) -> Result<AuthenticatedConnection, AuthError> {
        // Validate and return a type that can perform privileged operations.
    }
}
```

Do not force typestate into simple CRUD code. Use it when transition safety or capability restriction materially improves.

## 2. Ownership and borrowing

Choose ownership according to lifecycle.

- Borrow data for short synchronous computation.
- Own data that crosses task, thread, callback, or storage boundaries.
- Use `Arc` only for actual shared ownership.
- Use `Cow` only when both borrowed and owned paths are common and useful.
- Clone small values when it simplifies safe ownership and profiling shows no issue.
- Avoid cloning large request graphs to work around an unclear state owner.
- Prefer iterators when they improve clarity; prefer explicit loops when control flow is easier to audit.

Do not add lifetime parameters to public architecture without a clear benefit. A small owned boundary type is often easier and safer across Tauri IPC and async tasks.

Keep secret-bearing values out of casual `Clone`, `Debug`, and serialization. Wrap them in a type that makes exposure deliberate.

## 3. Error and panic policy

Use `Result` for expected failures and reserve panic for broken internal invariants that cannot be recovered safely.

- Define domain errors near the invariant.
- Add context at infrastructure boundaries while preserving the original cause.
- Map internal errors to stable, sanitized interface errors at IPC or API boundaries.
- Distinguish invalid input, unauthorized, conflict, unavailable, timeout, canceled, and internal failure when callers need different behavior.
- Avoid string matching for program logic.
- Avoid converting to a generic dynamic error before policy decisions are complete.

For application binaries, a general error context crate can be appropriate at the outer diagnostic boundary. For reusable libraries and stable internal contracts, prefer explicit error enums. Reuse repository conventions rather than adding a second error stack.

Do not use `unwrap` or `expect` for user input, I/O, IPC, parsing, lock acquisition, network responses, or platform operations. In tests, use them when failure already produces useful local evidence and does not hide the assertion intent.

If startup requires an invariant, return a contextual startup error when an operator can correct it. Panic only when continuing would violate assumptions and no recovery path exists.

## 4. Async and concurrency

Use async for waiting, not as a default function color.

- Keep pure domain computation synchronous.
- Make an API async only when its contract waits on async I/O, synchronization, or cancellation.
- Isolate blocking calls with the runtime-supported blocking mechanism or a bounded worker.
- Avoid holding locks or borrowed guards across `.await`.
- Bound task creation and queues.
- Retain task handles or supervise tasks.
- Define cancellation and cleanup for every long-running operation.
- Use structured ownership so a service cannot outlive the resources it controls.

Choose synchronization by invariant:

- Mutex for short exclusive mutation
- Read-write lock only when measured read concurrency justifies its complexity
- Atomic for one small invariant with clear memory-order reasoning
- Channel for transferring ownership or serializing a state machine
- Semaphore for bounded concurrency
- Watch or broadcast mechanism for state snapshots or fan-out when lag behavior is defined

Do not choose an async mutex merely because the surrounding function is async. Choose it when the guard must legitimately remain across await; redesign first.

Avoid detached tasks that can fail silently. Capture errors through a supervisor, channel, join handle, or structured log and metric.

## 5. Serialization and boundary types

Separate wire DTOs from domain types when validation, compatibility, privacy, or representation can differ.

- Define maximum sizes before deserializing deeply nested or large values where possible.
- Validate strings into enums, IDs, URLs, paths, and bounded collections.
- Use explicit serde naming and representation for contracts that must remain stable.
- Decide whether unknown fields are rejected or ignored based on compatibility requirements. `deny_unknown_fields` improves strictness but can block additive compatibility.
- Avoid untagged enums for security-sensitive ambiguous input unless cases are provably distinct.
- Avoid flattening that hides privilege-bearing fields.
- Use custom serialization for secrets and types whose `Debug` or `Serialize` exposure is unsafe.
- Test round-trip and old-fixture compatibility.

Do not serialize internal error chains, OS errors, or platform paths directly to the frontend.

## 6. Traits, generics, and abstraction

Create a trait when at least one is true:

- The application layer needs to invert a dependency on infrastructure.
- Multiple implementations exist or are planned by a concrete requirement.
- A test double materially improves deterministic testing.
- The trait defines a stable capability boundary.

Do not create traits for every struct. Do not hide a simple concrete type behind dynamic dispatch without a reason.

Keep traits small and behavior-oriented. Avoid a broad repository or service trait with unrelated methods. Put traits near the consumer that needs the abstraction.

Choose static dispatch for performance and type composition when code size remains acceptable. Choose dynamic dispatch for heterogeneous runtime implementations or to reduce generic surface. Measure before claiming either is faster in the application.

Avoid generic parameters that leak through many layers only to select one concrete implementation at startup. A narrow trait object at the composition boundary may be clearer.

## 7. Unsafe Rust and FFI

Use safe Rust by default.

For each unsafe block or implementation:

- State the exact invariant in a `SAFETY` comment.
- Prove pointer validity, alignment, initialization, lifetime, aliasing, and thread-safety assumptions.
- Minimize the unsafe region and expose a safe wrapper.
- Define ownership transfer and destructor responsibility for FFI values.
- Define behavior for null, error codes, partial writes, callbacks, panics, and unwind across the boundary.
- Use C-compatible representations only where required and verified.
- Test all supported architectures and ABI variants.
- Run Miri or sanitizers when applicable.

Never let a panic unwind across an FFI boundary unless the ABI and implementation explicitly support it. Convert callbacks and errors at the boundary.

Do not mark a type `Send` or `Sync` manually without a written proof of every contained resource and operation.

## 8. Performance and allocation

Use profiling before low-level changes.

- Remove algorithmic, I/O, serialization, and contention bottlenecks before individual allocations.
- Prefer slices and iterators for read-only bulk work.
- Reserve collection capacity when size is known and the path is material.
- Stream large data instead of collecting it.
- Avoid repeated UTF-8, JSON, and base64 transformations across IPC.
- Keep hot structs compact only when cache behavior is measured.
- Use `Arc<str>`, bytes, arenas, or interning only when retention and access patterns justify them.
- Avoid unsafe, lock-free, SIMD, and custom allocators without clear benchmark evidence and maintenance ownership.

Benchmark with release settings and realistic data. Include memory and correctness checks.

## 9. Cargo features and workspace design

Use features for additive capabilities, not mutually exclusive global modes when avoidable.

- Keep default features minimal and suitable for common production use.
- Document feature combinations and test supported combinations.
- Avoid feature flags that silently change security policy or data format.
- Avoid duplicate versions of heavy runtimes and native libraries without justification.
- Put shared dependency versions and lints at workspace level when repository policy supports it.
- Define MSRV if consumers or build environments depend on it.
- Keep build scripts small, deterministic, and free of unnecessary network or environment dependence.

Review the complete feature graph of Tauri plugins and platform crates. A disabled UI feature may still compile privileged backend capability unless dependencies and permissions are also removed.

## 10. Readability and review checklist

Before delivery, confirm:

- Types communicate units, identity, state, and trust.
- Public visibility is minimal.
- Error categories support caller decisions.
- Runtime paths do not panic on external failure.
- Shared state has a named owner and lifecycle.
- Async work is bounded, cancellable, and observable.
- Locks are short and not accidentally held across await.
- Boundary DTOs are validated and compatibility-tested.
- Unsafe code has a local proof and targeted analysis.
- Dependencies and features are justified.
- Performance changes have evidence.
- Comments explain invariants and decisions rather than syntax.
