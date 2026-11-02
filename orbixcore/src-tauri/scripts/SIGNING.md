# Signing & notarizing the macOS build

The release workflow (`.github/workflows/release.yml`) signs and notarizes the `.app`
automatically **when the Apple secrets below are present**. With no secrets it
builds unsigned exactly as before, so forks and dev builds keep working - an
unsigned build just trips Gatekeeper ("Apple could not verify this app is free
of malware") and users would need `xattr -cr`.

## One-time prerequisites (Apple Developer Program)

1. Enroll in the Apple Developer Program and note your **Team ID**
   (developer.apple.com/account → Membership).
2. Create a **Developer ID Application** certificate, export it from Keychain
   as a `.p12` with a password.
3. Create an **app-specific password** for notarization
   (appleid.apple.com → Sign-In and Security → App-Specific Passwords).

## Repo secrets to add (Settings → Secrets and variables → Actions)

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | `base64 -i DeveloperID.p12` (the whole file, base64) |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password |
| `KEYCHAIN_PASSWORD` | any random string (a throwaway CI keychain password) |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | the app-specific password from step 3 |
| `APPLE_TEAM_ID` | your 10-character Team ID |

Signing needs the first four; notarization + stapling needs all seven. When
`APPLE_CERTIFICATE` is empty the import step is skipped and the build is
unsigned - no failure.

`security find-identity -p codesigning -v` locally prints the exact
`APPLE_SIGNING_IDENTITY` string to copy.

## Verify on the first signed build

- **Hardened runtime + entitlements.** Tauri v2 enables hardened runtime when
  signing for Developer ID. `entitlements.plist` currently grants audio-input
  and screen-capture. If the first notarized build fails to launch the local
  model, the spawned `llama-server` (a separately-downloaded binary, not
  bundled) may need `com.apple.security.cs.allow-jit` /
  `allow-unsigned-executable-memory` added here. Confirm with a real run before
  publishing - this is untested until a Developer ID cert exists.
- **Bundled helpers are signed too.** `orbix`, `bob`, and `orbix-speak` ship
  as sidecars (`bundle.externalBin`, staged by `npm run stage` as
  `binaries/<name>-<target-triple>`), so the bundler puts them in
  `Contents/MacOS` and signs them with the app. They must not go back to
  `bundle.resources`: the bundler never signs resources, and notarization
  rejects any unsigned Mach-O in the bundle. Check with
  `codesign --verify --deep --strict` on the `.app` and `spctl -a -vvv` on
  the DMG.
- **Apple Events.** `entitlements.plist` grants
  `com.apple.security.automation.apple-events` and `Info.plist` carries
  `NSAppleEventsUsageDescription`; a hardened-runtime app cannot drive
  Outlook, Calendar, or the browsers over `osascript` without them.
- **Staple check:** `xcrun stapler validate` on the final DMG.

## Local one-shot alternative

If you'd rather sign from your Mac instead of CI, the same env vars drive a
local `npm run tauri:build` (which also stages the CLI + Swift helper). Store
notarization creds once with
`xcrun notarytool store-credentials` and export `APPLE_SIGNING_IDENTITY` before
building.

# Signing the Windows build (Authenticode)

Unsigned installers trip SmartScreen ("Windows protected your PC"). Signing
the app executable, the NSIS installer with its uninstaller, and the MSI
replaces "Unknown publisher" with the verified company name. It does not
silence SmartScreen on day one: reputation builds with downloads, for every
kind of certificate. Microsoft removed the instant reputation EV certificates
used to carry (March 2024), so EV buys nothing for SmartScreen any more. What
signing does buy is that reputation carries from one release to the next
under the same publisher identity; unsigned builds start from zero every time.

The wiring is in place and **off until a certificate exists**. Nothing here
has signed a real build yet: no certificate is enrolled. Treat the recipes
below as the plan to verify on the first signed build.

## How it is wired

`scripts/release-config.mjs` writes `release-overrides.json` from the
environment and the release workflow builds with
`tauri build --config release-overrides.json`. With nothing set the file is
`{}` and the build is unsigned, exactly as before.

| Variable | Effect |
|---|---|
| `WINDOWS_SIGN_COMMAND` | Sets `bundle.windows.signCommand`: Tauri runs it once per file, `%1` is the file. A plain string is split on spaces; for arguments with spaces use JSON, `{"cmd":"tool","args":["-d","Orbix Core","%1"]}`. |
| `WINDOWS_CERT_THUMBPRINT` | Sets `certificateThumbprint` + SHA-256 + an RFC 3161 timestamp: `signtool` with a certificate in the Windows store. Local builds with a hardware token only. |
| `WINDOWS_TIMESTAMP_URL` | Timestamp server for the thumbprint route (default `http://timestamp.digicert.com`). |

In CI, `WINDOWS_SIGN_COMMAND` and `WINDOWS_SIGN_TOOL_INSTALL` are repository
**variables** (Settings > Secrets and variables > Actions > Variables): they
name a tool and an account, not a credential. The credentials are secrets,
passed to the build step as environment variables. When
`WINDOWS_SIGN_COMMAND` is set, a "Verify Windows signatures" step fails the
release unless every installer carries a valid signature, so a signer that
silently did nothing cannot ship.

## Choosing a certificate

Since June 2023 a new OV or EV code signing key must live on hardware or in
a cloud HSM. There is no `.pfx` to put in a GitHub secret any more.

| Route | Fits CI | Notes |
|---|---|---|
| **Azure Artifact Signing** (named Trusted Signing until 2025) | Yes | **Recommended.** From $9.99 a month, no certificate purchase, no token, Microsoft's own recommendation for apps outside the Store. Public Trust is open to organizations in the US, Canada, the EU, the UK, Switzerland, Norway, Australia, New Zealand, Japan, South Korea, Singapore, and Israel, and to individuals in the US and Canada only. There is no minimum company age: the "three years of history" rule was a preview-era requirement that still circulates (checked against the quickstart prerequisites and a Microsoft staff answer, 2026-09). |
| **Cloud HSM certificate** (SSL.com eSigner, DigiCert KeyLocker) | Yes | The fallback if Microsoft's validation fails. A normal OV certificate held by the CA, signed through the CA's CLI, a few hundred a year. Do not pay for EV: see above. |
| **Hardware token** (USB) | No | Sign on the build PC only; the token must be plugged in. Use `WINDOWS_CERT_THUMBPRINT`. Does not fit a CI release. |

## Route A: Azure Artifact Signing

Before starting:

- A **paid** Azure subscription (pay-as-you-go is enough). Free, trial, and
  sponsored subscriptions are refused. Billing is per month and not pro-rated,
  so create the account when ready to validate, not before.
- Validation takes 1 to 20 business days and allows three document attempts.
  It checks public records, so line them up first: the company name and
  address exactly as in the commercial register, a website on the company's
  domain, a monitored mailbox on that domain as the primary email, the domain
  registered to the company (keep the registrar invoice), and a registration
  extract issued within the last 12 months. The person representing the
  company verifies with a government ID through Microsoft Authenticator.
- The certificate subject is the legal company name and cannot be customized.
  That exact string is what users see as the publisher.
- Certificates are short-lived (days). The timestamp is what keeps a signature
  valid afterwards, so never sign without one
  (`http://timestamp.acs.microsoft.com`; the signing tools add it).

Then:

1. In Azure: register the `Microsoft.CodeSigning` resource provider, create an
   Artifact Signing account (Basic tier, a European region such as West
   Europe, endpoint `https://weu.codesigning.azure.net`), give yourself the
   "Artifact Signing Identity Verifier" role, complete the organization
   identity validation (Public), create a **Public Trust**
   certificate profile. Create an App Registration with a client secret and
   give it the "Artifact Signing Certificate Profile Signer" role on the
   account.
2. Repository secrets: `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`,
   `AZURE_CLIENT_SECRET`.
3. Repository variables:
   - `WINDOWS_SIGN_TOOL_INSTALL`: `cargo install artifact-signing-cli --locked`
     (the community CLI Tauri's documentation uses; it was published as
     `trusted-signing-cli` before the service was renamed. Flags `-e -a -c -d`,
     auth through the three AZURE_* secrets.)
   - `WINDOWS_SIGN_COMMAND`:
     `{"cmd":"artifact-signing-cli","args":["-e","https://weu.codesigning.azure.net","-a","<account>","-c","<profile>","-d","Orbix Core","%1"]}`

   The CLI downloads Microsoft's signing dlib on first use and calls the
   Windows SDK `signtool` with the Microsoft timestamp server, so the
   signature carries a countersignature. The client secret expires; set a
   reminder to rotate it before CI starts failing with 401.

## Route B: a cloud HSM certificate

Install the CA's signing CLI in `WINDOWS_SIGN_TOOL_INSTALL`, point
`WINDOWS_SIGN_COMMAND` at it with `%1` for the file, and store its login in
`WINDOWS_SIGN_USERNAME`, `WINDOWS_SIGN_PASSWORD`, and (for eSigner)
`WINDOWS_SIGN_TOTP_SECRET`; the build step exports all three. If the CLI wants
differently named variables, add them to the build step's `env:` block in
`.github/workflows/release.yml`.

## Route C: a hardware token, locally

```pwsh
# The thumbprint of the certificate on the token:
Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Format-List Subject, Thumbprint
$env:WINDOWS_CERT_THUMBPRINT = "<thumbprint>"
cd orbixcore
node src-tauri/scripts/release-config.mjs release-overrides.json
npm run stage
npm run tauri -- build --config release-overrides.json
```

`signtool.exe` comes with the Windows SDK (part of the Visual Studio Build
Tools C++ workload).

## Verify on the first signed build

- `Get-AuthenticodeSignature` on the installer, the MSI, and the installed
  `orbixcore.exe` reports `Valid`, with a timestamp countersignature (without
  one the signature dies with the certificate).
- The bundled `orbix.exe` and `bob.exe` are sidecars (`bundle.externalBin`),
  so the bundler runs the sign command on them too; confirm with
  `Get-AuthenticodeSignature` on the installed copies.
- The updater: a signed build must still pass the minisign check in
  `UPDATER.md`; the two signatures are independent.
- Download the installer through a browser on a clean Windows VM and note what
  SmartScreen says. That, not `signtool verify`, is what users meet.
