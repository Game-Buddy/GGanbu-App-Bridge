# Dependency advisory exceptions

Exceptions apply to individual advisory IDs. New vulnerabilities still fail both
`cargo audit` and `cargo deny`. The policies are in `.cargo/audit.toml` and
`src-tauri/deny.toml`.

## RUSTSEC-2026-0387: crypto-bigint

Owner: GameBuddy bridge maintainers. Review by **2026-12-01**, and whenever the
OPAQUE dependency or cipher suite changes.

The [advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) concerns
secret-dependent arithmetic in `crypto-bigint`. The locked dependency path is
`opaque-ke 4.0.1 -> elliptic-curve 0.13.8 -> crypto-bigint 0.5.5`, also reached
through `opaque-ke -> voprf 0.5.0 -> elliptic-curve`.

The application fixes both the OPRF and TripleDH groups to Ristretto255 in
`src-tauri/src/security/opaque.rs`. The inspected implementations are:

- [OPAQUE Ristretto255](https://github.com/facebook/opaque-ke/blob/v4.0.1/src/key_exchange/group/ristretto255.rs)
  uses `curve25519_dalek::scalar::Scalar` and `RistrettoPoint` for secret keys
  and Diffie-Hellman multiplication.
- [VOPRF Ristretto255](https://github.com/cfrg/voprf/blob/v0.5.0/src/group/ristretto.rs)
  also uses those Dalek scalar and point types. Its `elliptic_curve::hash2curve`
  use expands messages into byte arrays; it does not perform big-integer scalar
  arithmetic.

The affected secret-integer operations are therefore not used by this suite.
The dependency remains present because the upstream crates support other groups.
Disabling optional OPAQUE features does not remove its mandatory elliptic-curve
dependency. The released OPAQUE 4.0.1 and 4.1 prerelease manifests still require
elliptic-curve 0.13; substituting crypto-bigint 0.7 is not a compatible lockfile
update. Switching to unreleased upstream crypto APIs would also require a
separate compatibility review of persisted credentials and the browser client.

The test `ristretto_suite_preserves_crypto_bigint_exception_assumptions` guards
both application group selections. This exception records non-reachability; it
is not a claim that crypto-bigint 0.5.5 is patched. Reassess or remove it before
adding other groups or direct big-integer operations. Remove both policy entries
when a compatible upstream release removes the affected dependency. Cargo deny's
`unused-ignored-advisory = "deny"` helps catch obsolete exceptions.

## RUSTSEC-2024-0429: GLib

The existing exception in `src-tauri/deny.toml` concerns the GTK3 dependency
inherited from Tauri. The application does not call `glib::VariantStrIter`.
Its existing review deadline is **2026-11-30**. Keep reviewing the supported
Tauri/GTK dependency chain for a patched upgrade.
