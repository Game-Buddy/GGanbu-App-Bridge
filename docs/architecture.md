# Application architecture

GGanbu App Bridge is a Tauri desktop application. The WebView is an untrusted
presentation and IPC client; Rust owns security policy, persistence, protocol
validation, operating-system input, and the local HTTP bridge.

## Source layout

```text
src-tauri/src/
├── lib.rs                 # crate module surface and Tauri entrypoint
├── app.rs                 # builder, setup, managed state, shutdown
├── commands/              # thin Tauri IPC adapters
│   ├── bridge.rs          # snapshots, events, host address
│   ├── devices.rs         # device rename and revocation
│   ├── mappings.rs        # native picker and default preset commands
│   ├── pairing.rs         # desktop pairing lifecycle
│   └── server.rs          # bridge server lifecycle
├── persistence.rs         # atomic file replacement and process instance lock
├── mappings.rs            # preset parsing, embedded assets, import limits
├── actions.rs             # allowlisted action catalog and resolution
├── config.rs              # build-time and runtime origin configuration
├── tests/                 # crate-local unit tests, kept separate from production files
│   ├── security_*.rs      # security state, storage, and OPAQUE tests
│   └── server.rs          # Axum transport and protocol-boundary tests
├── state.rs               # UI snapshot state and publication model
├── server.rs              # Axum transport, middleware, lifecycle serving
├── protocol.rs            # HTTP protocol DTOs, authentication, and actions
├── security/              # pairing, sessions, OPAQUE, persistence
└── keyboard.rs            # platform-specific keyboard adapters

src-tauri/tests/           # black-box integration tests for the public crate API
```

## Boundary rules

- `app.rs` wires the application. It does not implement product behavior.
- `commands/` contains request/response mapping and calls application/domain
  functions. Commands must validate inputs before crossing into privileged
  code.
- `mappings.rs`, `security/`, `protocol.rs`, `server.rs`, and `keyboard.rs`
  contain the behavior that should remain testable without a WebView.
- Unit tests live in `src-tauri/src/tests/` and are declared as nested modules
  by their owning production module. This keeps tests physically separate while
  preserving private access for parser, security, and platform invariants.
- Tests that exercise only the public crate surface or multiple compiled
  components belong in `src-tauri/tests/` as black-box integration tests.
- Frontend-provided file paths are not accepted as a generic filesystem
  command. The native picker selects and loads a mapping in one Rust command;
  the default mapping has a separate command.
- The local bridge remains authenticated and origin-constrained independently
  of Tauri IPC capabilities or CORS.

## Startup and request flow

```text
Tauri builder
  -> app setup
     -> load security state
     -> publish initial snapshot
     -> validate allowed origins
  -> frontend invokes a command
     -> commands/<area>.rs validates and delegates
        -> domain/service module
           -> state/event or typed result

Browser client
  -> Axum middleware
     -> origin, content type, body-size, and rate limits
     -> protocol authentication and replay checks
     -> allowlisted action / keyboard adapter
```

Changes that add a privileged operation should normally add a focused command
module function, a negative test at the Rust boundary, and an update to this
document when the trust boundary changes.

## Cancellation and persistence

The application holds an OS file lock on `instance.lock` before loading security
state. It permits one process per application data directory. Security mutations
serialize through a separate mutation mutex; disk writes finish before the in-memory
commit, without holding the shared security lock during I/O.

Server start and stop share an async lifecycle mutex. Start binds before returning.
Stop cancels keyboard admission, clears sessions and pairing, and awaits the listener
task. HTTP draining is limited to two seconds. Window exit awaits this same stop path.

At most eight keyboard operations may be pending, including the active operation.
Admission happens before spawning blocking work. Commands expire after 15 seconds and
check cancellation and session validity inside the execution lock and before each
key press. Wayland permission acquisition has the same deadline. Key releases are
attempted even if cancellation happens after a press. The 35 ms hold is retained;
there is no intentional delay before pressing.

Controls import validates the complete catalog before atomic replacement. Saved
controls include their source name and are restored without rewriting on startup.
Embedded presets are extracted into `presets/<application-version>/assets`; imports
remain separate from application-owned presets.

## UI events and integration checks

`get_bridge_snapshot` returns the complete bounded history. State events omit
`actionHistory`; the UI appends a newer `lastAction`, retains at most 200 entries,
and requests a complete snapshot when revisions skip. Revisions prevent older
snapshots or events from replacing newer state. Pairing and session expiry are
published once per second while the server runs. The countdown updates only its
own component and stops at zero.

Run `pnpm test:bridge` to build a loopback-only Rust protocol fixture and exercise it
with the JavaScript OPAQUE client. The fixture stores no credentials on disk and
uses an executor that sends no operating-system input. CI runs this check on Linux
and Windows in addition to platform Rust tests. Real-game timing and permission
dialog behavior still need native acceptance testing on Windows, X11 and Wayland.
