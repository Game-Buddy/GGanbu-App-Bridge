# Performance Engineering Guide

## Contents

1. Performance contract
2. Measurement method
3. Startup and shutdown
4. Async, blocking, and concurrency
5. State and locking
6. IPC and event flow
7. Memory and allocation
8. Filesystem, database, and network I/O
9. Frontend and WebView
10. Release profiles and binary size
11. Benchmark reporting

## 1. Performance contract

Define performance in user-observable terms. Examples:

- Cold and warm startup time
- Time to first usable window
- Command latency at p50, p95, and p99
- Event-to-render latency
- Throughput under a defined workload
- Idle and active CPU usage
- Resident memory after startup and after sustained use
- Queue depth and dropped-event rate
- Installer and binary size
- Shutdown time and orphan-process count

Set a target or budget before optimizing. A performance change is incomplete without a correctness and security check.

Do not optimize source-code aesthetics. Optimize a measured bottleneck that matters to the product.

## 2. Measurement method

Use a repeatable method:

1. Define hardware, operating system, build profile, toolchain, dataset, and workload.
2. Build in release mode when measuring production behavior.
3. Separate cold-cache and warm-cache runs.
4. Warm up JIT-like frontend and system effects when the metric requires it.
5. Collect enough samples to show variance and outliers.
6. Record baseline before editing.
7. Change one dominant factor at a time where feasible.
8. Compare identical workloads.
9. Re-run correctness, security, and reliability tests.
10. Report regressions and tradeoffs, not only the winning metric.

Use profilers and traces before micro-optimizing. Choose platform-appropriate tools such as flame graphs, Instruments, Windows Performance Recorder, perf, heap profilers, browser performance tools, or application spans.

## 3. Startup and shutdown

Keep the critical startup path small.

- Create the window and usable UI before non-essential network or disk work when product behavior permits.
- Lazy-initialize optional plugins and services.
- Move blocking initialization away from the UI or async runtime thread.
- Avoid synchronous network calls at startup.
- Cache only data with a clear invalidation policy.
- Load large datasets incrementally.
- Avoid registering plugins, global shortcuts, watchers, and background workers that the current session does not need.
- Measure frontend bundle parse and render time separately from Rust initialization.

Define shutdown order:

1. Stop accepting new work.
2. Signal cancellation.
3. Drain or reject queued work according to policy.
4. Flush required state.
5. Stop local listeners and child processes.
6. Join supervised tasks within a timeout.
7. Record incomplete cleanup safely.

A faster exit that corrupts state or leaves privileged child processes is not an improvement.

## 4. Async, blocking, and concurrency

Classify work before choosing execution:

- Short CPU work: execute directly when it cannot starve the runtime.
- Long CPU work: use a bounded worker pool or blocking task mechanism.
- Blocking filesystem or foreign API: isolate from async executor threads.
- Async network or timer work: use async APIs and cancellation.
- Serialized device or state-machine access: use one owner task and message passing.

Rules:

- Do not call blocking APIs from async runtime threads without isolation.
- Do not create an unbounded task per event.
- Bound worker concurrency and queue depth.
- Define backpressure: block producer, reject, drop oldest, drop newest, or coalesce.
- Propagate cancellation through nested operations.
- Retain and observe task handles or supervise tasks centrally.
- Apply timeouts at the boundary that can recover safely.
- Avoid retry storms; use bounded retries with jitter where appropriate.
- Preserve request ordering only where the product requires it.

Concurrency can reduce latency or increase throughput, but it also expands race, shutdown, memory, and attack surfaces. Add it only with a measurable need.

## 5. State and locking

Measure lock wait and hold time before replacing synchronization primitives.

Prefer:

- Immutable state
- One owner for mutable resources
- Short critical sections
- Per-resource locks instead of a global lock
- Read snapshots for frequently read, infrequently changed state
- Atomics only for simple, well-documented invariants

Avoid:

- Holding a lock across `.await`
- Calling user code, frontend emission, logging sinks, or blocking I/O under a lock
- Nested locks without a documented order
- Repeatedly cloning large state merely to escape lock design
- Lock-free structures without a demonstrated contention problem and expert review

For channels and queues, expose metrics for depth, enqueue wait, dropped items, and processing latency.

## 6. IPC and event flow

Tauri IPC crosses a serialization and process or WebView boundary. Treat frequency and payload size as design inputs.

- Prefer coarse application operations over chatty property-level commands.
- Batch or coalesce high-frequency updates.
- Send deltas when they are smaller and recovery from missed updates is defined.
- Avoid repeated large JSON payloads and base64 encoding of large binary data.
- Use files, streams, channels, or a purpose-built local protocol for large or sustained data when appropriate.
- Paginate large collections.
- Version message envelopes when frontend and backend may update independently.
- Include sequence or snapshot identifiers when ordering matters.
- Unsubscribe event listeners and release resources when views unmount.

Measure serialization time, payload bytes, queueing, command execution, and frontend render separately.

## 7. Memory and allocation

Start with retention and growth, not individual allocations.

Investigate:

- Unbounded collections, logs, queues, caches, and event history
- Tasks and event listeners that never terminate
- Large frontend state duplicated in Rust and JavaScript
- Repeated image, JSON, and string transformations
- Base64 expansion
- Accidental whole-file reads
- Reference cycles and retained closures in frontend code
- Multiple dependency stacks that duplicate buffers or runtimes

Set cache size and eviction policy. Use streaming for large data. Reuse buffers only when ownership remains clear and profiling shows value.

Do not remove a clone merely because it exists. The clone may be small, infrequent, or necessary for clean concurrency. Measure first.

## 8. Filesystem, database, and network I/O

Filesystem:

- Use buffered and streaming I/O for large content.
- Batch metadata operations when safe.
- Keep atomicity and durability semantics explicit.
- Avoid frequent polling when platform watchers are reliable and bounded.

Database:

- Measure query plans and round trips.
- Add indexes for measured query patterns.
- Use transactions around required atomic work, not around slow external I/O.
- Bound connection pools for a desktop workload.
- Paginate or stream large results.
- Avoid N+1 queries and repeated schema setup.

Network:

- Reuse clients and connections.
- Bound timeout, retry, response size, and concurrency.
- Cache only with clear freshness and privacy rules.
- Avoid network work on the startup critical path unless required.
- Compress only when CPU and latency tradeoffs are measured.

## 9. Frontend and WebView

Profile component renders and browser tasks before adding memoization.

- Keep frequently changing state close to consumers.
- Avoid replacing large object graphs for tiny updates.
- Virtualize long lists.
- Throttle or coalesce telemetry and high-frequency events.
- Use animation-frame scheduling for visual updates.
- Split routes and heavy optional features when bundle size matters.
- Avoid synchronous layout thrashing.
- Move expensive pure transformations out of render paths.
- Use web workers only when transfer cost and complexity are justified.
- Clean up timers, listeners, subscriptions, and object URLs.

For React, memoization is a tool, not a default. Confirm that dependency stability and render cost make it beneficial.

## 10. Release profiles and binary size

Use Cargo profile changes only with measured goals.

Evaluate:

- `opt-level` for speed versus size
- thin or fat LTO versus link time
- `codegen-units` versus compile time and runtime optimization
- symbol stripping versus crash diagnostics
- `panic = "abort"` versus unwind behavior and diagnostics
- debug information retained for production symbolication
- dependency features and duplicate crate versions

Keep a debuggable release process. A smaller binary that prevents useful crash analysis may be a poor trade.

Measure packaged application and installer size, not only the Rust executable. Frontend assets, WebView resources, sidecars, and platform bundles can dominate.

## 11. Benchmark reporting

Report performance work with:

```text
Metric:
Target or budget:
Environment:
Build profile and revision:
Workload and dataset:
Baseline samples:
Changed samples:
Relative and absolute difference:
Variance or confidence:
Correctness and security checks:
Tradeoffs:
Remaining bottleneck:
```

Do not use a single run as proof. Do not compare debug and release builds. Do not omit a regression in memory, CPU, startup, binary size, or reliability that accompanied a latency gain.
