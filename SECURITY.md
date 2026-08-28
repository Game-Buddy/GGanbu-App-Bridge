# Security policy

## Supported versions

Security fixes are applied to `main` and the latest stable release. Update to the latest release before reporting issues that affect older versions.

| Version            | Supported           |
| ------------------ | ------------------- |
| `main`             | Yes                 |
| `v0.5.2` and later | Yes                 |
| Older releases     | Upgrade recommended |

## Reporting a vulnerability

Do not open a public issue, discussion, or pull request for a suspected vulnerability.

Use GitHub's [private vulnerability reporting form](https://github.com/Game-Buddy/GGanbu-App-Bridge/security/advisories/new). Include, when available:

- the affected version or commit;
- the operating system and architecture;
- reproducible steps or a proof of concept;
- the security impact;
- any suggested mitigation;
- whether the report may be credited publicly.

Do not include passwords, pairing codes, session credentials, private keys, or other secrets in the report. If the report involves an exposed credential, revoke or rotate it first when possible.

If the private reporting form is unavailable, use the [Game Buddy contact page](https://gganbu.app/contact) to request a private security channel. Do not include technical details until a private channel has been confirmed.

Maintainers will acknowledge a complete report as soon as practical, validate its impact, coordinate a fix and disclosure timeline, and credit reporters who request attribution. Please allow time for a patched release before publishing details.

## Release integrity

Official Linux builds are published through [GitHub Releases](https://github.com/Game-Buddy/GGanbu-App-Bridge/releases/latest) with checksums and detached signatures. Official Windows builds are distributed through the [Microsoft Store](https://apps.microsoft.com/detail/9nfnnsx3pc7j). GitHub Actions MSIX artifacts are for Store submission and QA, not end-user installation.

Follow [release signing and verification](./docs/security/release-signing.md) to verify Linux assets. Never commit or share private signing material.
