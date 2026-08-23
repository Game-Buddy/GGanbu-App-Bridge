# Security policy

## Supported versions

GGanbu App Bridge has not published its first release. During bootstrap, security fixes are applied to `main`. After releases begin, this table will identify the supported release lines.

| Version  | Supported |
| -------- | --------- |
| `main`   | Yes       |
| Releases | None yet  |

## Reporting a vulnerability

Do not open a public issue, discussion, or pull request for a suspected vulnerability.

Use GitHub's [private vulnerability reporting form](https://github.com/Game-Buddy/GGanbu-App-Bridge/security/advisories/new). Include:

- the affected version or commit;
- the operating system and architecture;
- reproducible steps or a proof of concept;
- the security impact;
- any suggested mitigation;
- whether the report may be credited publicly.

If the private reporting form is unavailable, use the [Game Buddy contact page](https://gganbu.app/contact) to request a private security channel. Do not include technical details until a private channel has been confirmed.

Maintainers will acknowledge a complete report as soon as practical, validate its impact, coordinate a fix and disclosure timeline, and credit reporters who request attribution. Please allow time for a patched release before publishing details.

## Release integrity

Official Linux builds are published only through this repository's GitHub Releases page. Official Windows builds are published only through the Microsoft Store; unsigned Actions artifacts are for Store submission and QA, not end-user installation. Published assets include checksums, signatures, and provenance where configured. Never commit or share private signing material.
