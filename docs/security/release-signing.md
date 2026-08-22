# Release signing

Desktop releases are published only from protected `v*.*.*` tags by `.github/workflows/release.yml`. The workflow rejects tags that do not match the version in `VERSION`, rejects tags that are not reachable from `main`, refuses to overwrite an existing GitHub Release, verifies that all application version files are synchronized, builds installers in CI, isolates signing credentials from build steps, and publishes SHA-256 checksums, detached signatures for the Linux package and source archive, an SPDX SBOM, and optional GitHub attestations.

The workflow cannot publish until the protected environment, the signing credentials, and the tag ruleset are configured.

## Repository setup before the first release

1. Maintain a GitHub environment named `release`.
2. Add the appropriate maintainer reviewers and prevent self-review.
3. Restrict deployments to protected `v*` tags.
4. Store all four signing values below as environment secrets.
5. Set the GitHub Actions release-environment variable `ENABLE_BUILD_ATTESTATIONS` to `true` only when GitHub artifact attestations are available for the repository; the workflow reads it as `vars.ENABLE_BUILD_ATTESTATIONS`.
6. Create an active tag ruleset for `refs/tags/v*` that restricts creation and blocks updates, force pushes, and deletion.
7. Publish the Linux public key and its full fingerprint through an independent trusted channel.
8. Test a prerelease before publishing a stable version.

Do not create placeholder secrets or commit signing material just to make the workflow pass.

## Required release environment secrets

- `LINUX_GPG_PRIVATE_KEY`: ASCII-armored private key used to create detached signatures for Linux AppImages.
- `LINUX_GPG_PASSPHRASE`: passphrase for that private key.
- `WINDOWS_CERTIFICATE_BASE64`: base64-encoded password-protected PFX code-signing certificate.
- `WINDOWS_CERTIFICATE_PASSWORD`: password for the PFX file.

The ordinary check workflow and `main` QA builds cannot access these secrets or publish stable binaries. Pull requests from forks do not receive release credentials.

## Preparing signing values

Export an existing Linux signing key on Linux:

```bash
gpg --armor --export-secret-keys <FULL_FINGERPRINT> > gganbu-app-bridge-private.asc
gpg --armor --export <FULL_FINGERPRINT> > gganbu-app-bridge-public.asc
```

Encode a production Windows PFX on Linux:

```bash
base64 -w 0 gganbu-app-bridge-signing.pfx > gganbu-app-bridge-signing.pfx.base64
```

Encode it in PowerShell on Windows:

```powershell
[Convert]::ToBase64String(
  [IO.File]::ReadAllBytes("C:\secure\gganbu-app-bridge-signing.pfx")
) | Set-Clipboard
```

Delete temporary private-key exports securely after the environment secrets are configured. A self-signed Windows certificate is suitable only for workflow testing and is not trusted by normal Windows installations.

## Verifying a release

Download all release files into one directory and verify the checksums:

```bash
sha256sum --check SHA256SUMS
```

Verify the detached Linux signature after importing the independently obtained public key:

```bash
gpg --verify "GGanbu App Bridge_"*.AppImage.asc "GGanbu App Bridge_"*.AppImage
```

Verify the matching source archive:

```bash
gpg --verify gganbu-app-bridge-*-source.tar.gz.asc +  gganbu-app-bridge-*-source.tar.gz
```

When attestations are enabled, verify provenance with GitHub CLI:

```bash
gh attestation verify "GGanbu App Bridge_"*.AppImage \
  --owner Game-Buddy \
  --repo GGanbu-App-Bridge
```

Windows installers are signed with Authenticode and timestamped by DigiCert. Verify the signature in the file properties or with `signtool verify` on a trusted Windows machine. macOS artifacts are not produced by this workflow.

## Release notes and rollback

GitHub generates initial notes from merged pull requests. Before marking a release stable, add upgrade instructions, known limitations, security-relevant changes, and rollback steps. Mark testing builds as prereleases. Rollback by directing users to the last known-good signed release; never move or overwrite an existing version tag.
