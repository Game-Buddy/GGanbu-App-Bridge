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
├── mappings.rs            # preset parsing, embedded assets, import limits
├── actions.rs             # allowlisted action catalog and resolution
├── config.rs              # build-time and runtime origin configuration
├── tests/                 # crate-local unit tests, kept separate from production files
│   ├── commands_server.rs # server-command lifecycle tests
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
