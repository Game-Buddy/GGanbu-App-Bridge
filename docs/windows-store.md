# Microsoft Store distribution

Windows builds are distributed through the Microsoft Store as MSIX packages. The Store signs the accepted package with the trusted certificate associated with the reserved product identity. The unsigned MSIX produced by GitHub Actions is a submission artifact, not a public installer, and must never be attached to a GitHub Release or offered directly to users.

This avoids maintaining a commercial code-signing certificate while giving users the normal trusted Store installation and update experience. Microsoft controls certification and reputation checks, so no packaging change can guarantee that a rejected or malicious build will be warning-free.

## One-time Partner Center setup

1. Create a [Windows developer account](https://learn.microsoft.com/en-us/windows/apps/publish/faq/open-developer-account) in Partner Center.
2. Reserve the product name `GGanbu App Bridge`.
3. Open the product's **Product management > Product identity** page and copy these values exactly:
   - Package/Identity/Name
   - Package/Identity/Publisher
   - Package/Properties/PublisherDisplayName
4. Add them as GitHub repository variables:

   ```bash
   gh variable set MSIX_IDENTITY_NAME --body '<Package/Identity/Name>'
   gh variable set MSIX_PUBLISHER --body '<Package/Identity/Publisher>'
   gh variable set MSIX_PUBLISHER_DISPLAY_NAME --body '<PublisherDisplayName>'
   ```

These values are public package metadata, not secrets. Use the exact values shown by Partner Center; changing punctuation, whitespace, or the distinguished name will make submission fail. The authoritative identity fields are described in [Microsoft's product identity documentation](https://learn.microsoft.com/en-us/windows/apps/publish/view-app-identity-details).

If the variables are absent, CI deliberately uses a development identity so the package structure can still be validated. The workflow summary and `submission-metadata.json` mark that artifact as non-submittable.

## Build and submit

For a stable release, the `Desktop release and Store package` workflow:

1. builds the Tauri executable with the production origin policy;
2. renders the Partner Center identity into `Package.appxmanifest`;
3. creates an unsigned MSIX with Microsoft's Windows App CLI;
4. records a SHA-256 checksum and identity metadata; and
5. uploads the files as the non-release `store-submission-windows-x86_64` Actions artifact for 90 days.

Download that artifact, confirm `storeReady` is `true` in `submission-metadata.json`, verify the checksum, and upload the `.msix` to the matching Partner Center product submission. Do not sign it first: Partner Center accepts the unsigned package and signs it during publication. Follow [Microsoft's MSIX package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements) when completing the listing and certification questionnaire.

The manifest declares only `runFullTrust`. This restricted capability is required because Tauri is a classic desktop application and the bridge must run native keyboard and local-server code. Explain that purpose in the submission notes if certification asks for justification.

## Version mapping

Store package versions require four numeric fields, the first field cannot be zero, and the fourth field must be zero. The packaging script therefore maps application SemVer as follows:

```text
MSIX = (SemVer major + 1).SemVer minor.SemVer patch.0
0.5.0 -> 1.5.0.0
1.2.3 -> 2.2.3.0
```

This mapping remains monotonic across stable SemVer releases. The MSIX workflow rejects prerelease versions because Store package versions cannot represent a prerelease without risking a collision with the later stable package. Submit stable versions to Partner Center.

## Local package validation

On Windows, after building the release executable and installing `winapp`, run:

```powershell
pnpm tauri build --no-bundle
pnpm msix:prepare
winapp pack target/msix/package `
  --manifest target/msix/package/Package.appxmanifest `
  --output target/msix/GGanbu-App-Bridge.msix `
  --skip-pri
```

An unsigned package cannot be installed locally without a trusted development signature. Generate and trust a development certificate only on a test machine; never publish a development-signed package. The [official Tauri packaging guide](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri) describes that optional local test flow.
