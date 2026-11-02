# Official Orbix Core Microsoft app registration

This is the one manual, vendor-side step behind "connect Outlook / Calendar."
You register **one** multi-tenant app in Azure **once**; its client ID ships
compiled into the app, and end users just click "Connect Microsoft" and sign in
with their own account. They never see Azure, never create a registration, never
paste a client ID.

Until the slot in `src-tauri/src/oauth.rs` (`OFFICIAL_CLIENT_IDS`) is filled,
Microsoft sign-in only works if a user manually drops their own client ID into
`~/Library/Application Support/ai.orbixcore.app/oauth_client_ids.json` - which is
exactly the friction we're removing.

---

## 1. Create the registration

Azure Portal → **Microsoft Entra ID** → **App registrations** → **New
registration**.

- **Name:** `Orbix Core`
- **Supported account types:** *Accounts in any organizational directory (any
  Microsoft Entra ID tenant - multitenant) **and** personal Microsoft accounts*.
  This matches the `/common/` endpoints the app uses and lets both work/school
  and personal Outlook.com users connect.
- **Redirect URI:** leave blank here; set it under Authentication (step 3).

Register, then copy the **Application (client) ID** (a GUID). It is **not** a
secret - it is safe to commit and ship.

---

## 2. Do NOT create a client secret

Orbix Core is a **public client** (a desktop app). It authenticates with PKCE
(loopback flow) and the device-code flow - no secret. `oauth.rs` sets
`client_secret: None` for Microsoft. Adding a secret would break, not help.

---

## 3. Authentication settings

App → **Authentication**:

- **Allow public client flows** → **Yes**. Required for the device-code flow
  (the one-click "Connect Microsoft" button in Triggers uses it) and for the
  loopback PKCE flow to work without a secret. If this is No you get errors.
- **Add a platform → Mobile and desktop applications**, and add:
  - `http://localhost` - Azure's loopback exception lets a public client use any
    port on `localhost`/`127.0.0.1`, which covers the app's dynamic
    `http://127.0.0.1:<port>/callback` redirect.
  - (Optional, harmless) the MSAL default
    `https://login.microsoftonline.com/common/oauth2/nativeclient`.

Both sign-in flows are already implemented in `oauth.rs`
(`start_oauth_flow` = loopback PKCE, `start_device_code_flow` = device code).

---

## 4. API permissions (Microsoft Graph, delegated)

App → **API permissions** → **Add a permission** → **Microsoft Graph** →
**Delegated permissions**. Add exactly what the app requests (see
`get_oauth_config("microsoft")` in `oauth.rs`):

| Permission | Why |
|---|---|
| `User.Read` | who is signed in |
| `Mail.Read` | read Outlook mail |
| `Mail.Send` | send mail as the user |
| `Calendars.Read` | read the Outlook calendar |
| `Calendars.ReadWrite` | create/update events |
| `Files.Read` | read OneDrive files |
| `offline_access` | refresh tokens (stay signed in) |
| `openid`, `profile`, `email` | sign-in / identity |

All are **delegated** and low-privilege, so an individual user can self-consent
on first sign-in. Do **not** click "Grant admin consent" - that's for a tenant
admin, and forcing it defeats the "any user just signs in" goal. (Some locked-
down tenants still require an admin to consent; that's their policy, not ours.)

---

## 5. Publisher verification (before public launch)

App → **Branding & properties** → verify the publisher. A verified publisher:

- removes the "unverified app" warning on the consent screen, and
- reduces the chance a tenant's admin-consent policy blocks sign-in.

Needs a Microsoft Partner Network (Cloud Partner) account with a verified domain
matching `orbixcore.ai`. This is a launch-blocker for a smooth first-run, not a
dev blocker.

---

## 6. Wire the client ID into the app

Once you have the Application (client) ID, put it in the compiled-in slot:

```rust
// src-tauri/src/oauth.rs
const OFFICIAL_CLIENT_IDS: &[(&str, &str)] = &[
    ("microsoft", "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"), // Orbix Core multitenant app id
];
```

Give me the GUID and I'll drop it in. It then ships with every build; the
per-user `oauth_client_ids.json` override still wins if someone wants their own.

Do the same for Google (`ORBIXCORE_GOOGLE_CLIENT_ID`) via Google Cloud Console
when you want one-click Google/Gmail/Calendar.

---

## Error decoder (already surfaced to users in the UI)

| AADSTS | Meaning | Fix |
|---|---|---|
| `AADSTS900971` / `AADSTS500113` | no matching reply/redirect address | add the Mobile-and-desktop redirect (step 3) + Allow public client flows |
| `AADSTS65001` | user/admin consent required | user consents on sign-in; locked tenants need an admin |
| `AADSTS700082` | refresh token expired | user re-connects (handled in `today.rs`) |

The user's earlier `AADSTS900971` was their personal registration missing the
loopback redirect - step 3 is the fix.
