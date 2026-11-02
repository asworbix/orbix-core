use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use rand::Rng;
use base64::Engine;

// ─────────────────────────────────────────────
//  OAuth2 PKCE Flow for Desktop Apps
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub connector_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>, // Some platforms (LinkedIn) require this
    pub scopes: Vec<String>,
    pub redirect_port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthResult {
    pub success: bool,
    pub connector_id: String,
    pub user_email: Option<String>,
    pub user_name: Option<String>,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────
//  PKCE helpers
// ─────────────────────────────────────────────

fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&hash)
}

fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.gen::<u8>()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes)
}

// ─────────────────────────────────────────────
//  OAuth provider configs
// ─────────────────────────────────────────────

/// Get OAuth config for a connector.
/// Returns None if the connector doesn't support OAuth login.
pub fn get_oauth_config(connector_id: &str) -> Option<OAuthConfig> {
    match connector_id {
        "microsoft" => Some(OAuthConfig {
            connector_id: "microsoft".to_string(),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            client_id: "ORBIXCORE_MICROSOFT_CLIENT_ID".to_string(),
            client_secret: None,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
                "offline_access".to_string(),
                "User.Read".to_string(),
                "Mail.Read".to_string(),
                "Mail.Send".to_string(),
                "Calendars.Read".to_string(),
                "Calendars.ReadWrite".to_string(),
                "Files.Read".to_string(),
            ],
            redirect_port: 0, // will be assigned dynamically
        }),
        "google" => Some(OAuthConfig {
            connector_id: "google".to_string(),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            client_id: "ORBIXCORE_GOOGLE_CLIENT_ID".to_string(),
            client_secret: None,
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
                "https://www.googleapis.com/auth/gmail.readonly".to_string(),
                "https://www.googleapis.com/auth/gmail.send".to_string(),
                "https://www.googleapis.com/auth/calendar.readonly".to_string(),
                "https://www.googleapis.com/auth/calendar.events".to_string(),
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
            ],
            redirect_port: 0,
        }),
        "github" => Some(OAuthConfig {
            connector_id: "github".to_string(),
            auth_url: "https://github.com/login/oauth/authorize".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            client_id: "ORBIXCORE_GITHUB_CLIENT_ID".to_string(),
            client_secret: None,
            scopes: vec![
                "read:user".to_string(),
                "user:email".to_string(),
                "repo".to_string(),
            ],
            redirect_port: 0,
        }),
        "slack" => Some(OAuthConfig {
            connector_id: "slack".to_string(),
            auth_url: "https://slack.com/oauth/v2/authorize".to_string(),
            token_url: "https://slack.com/api/oauth.v2.access".to_string(),
            client_id: "ORBIXCORE_SLACK_CLIENT_ID".to_string(),
            client_secret: None,
            scopes: vec![
                "channels:read".to_string(),
                "chat:write".to_string(),
                "users:read".to_string(),
            ],
            redirect_port: 0,
        }),
        "linkedin" => Some(OAuthConfig {
            connector_id: "linkedin".to_string(),
            auth_url: "https://www.linkedin.com/oauth/v2/authorization".to_string(),
            token_url: "https://www.linkedin.com/oauth/v2/accessToken".to_string(),
            client_id: "ORBIXCORE_LINKEDIN_CLIENT_ID".to_string(),
            client_secret: Some("ORBIXCORE_LINKEDIN_CLIENT_SECRET".to_string()),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
                "w_member_social".to_string(),
            ],
            // LinkedIn requires redirect_uri to EXACTLY match what's registered
            // in the LinkedIn Developer Console. Random ports won't work.
            // Use a fixed port so you only need to register once:
            // http://127.0.0.1:51237/callback
            redirect_port: 51237,
        }),
        _ => None,
    }
}

/// Check if a connector supports OAuth login flow
pub fn supports_oauth_login(connector_id: &str) -> bool {
    get_oauth_config(connector_id).is_some()
}

/// Load bundled credentials from credentials.json next to the binary, then merge user overrides.
/// Bundled credentials are set by the OrbixCore developer - end users never touch them.
/// Official Orbix app registrations, compiled into the binary. End
/// users never touch a cloud console: the vendor registers ONCE
/// (multi-tenant, public client - see docs/AZURE_APP_REGISTRATION.md)
/// and the id ships here. Empty entries are skipped so dev overrides
/// keep working until the official registration exists.
const OFFICIAL_CLIENT_IDS: &[(&str, &str)] = &[
    // Orbix Core's official multitenant Microsoft app (Azure Entra). A client
    // ID is public, not a secret. Ships compiled in so end users connect
    // Outlook / Calendar by signing in - they never touch Azure. A per-user
    // oauth_client_ids.json override still wins. See docs/AZURE_APP_REGISTRATION.md.
    ("microsoft", "a9678d6c-38d4-45f6-a4c5-9db53f371cd3"),
];

pub fn load_oauth_client_ids() -> HashMap<String, String> {
    let mut ids = HashMap::new();

    // 0. Compiled-in official ids (lowest precedence, always available)
    for (k, v) in OFFICIAL_CLIENT_IDS {
        if !v.is_empty() {
            ids.insert(k.to_string(), v.to_string());
        }
    }

    // 1. Load bundled credentials (shipped with the app)
    let bundled_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("credentials.json")));
    if let Some(bp) = bundled_path {
        if bp.exists() {
            if let Ok(data) = std::fs::read_to_string(&bp) {
                if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&data) {
                    for (k, v) in map {
                        if !v.is_empty() && !k.starts_with("_") {
                            ids.insert(k, v);
                        }
                    }
                }
            }
        }
    }

    // 2. Merge user overrides (from settings / Bob conversations)
    let user_path = crate::storage::base_dir().join("oauth_client_ids.json");
    if user_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&user_path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&data) {
                for (k, v) in map {
                    if !v.is_empty() {
                        ids.insert(k, v); // user overrides bundled
                    }
                }
            }
        }
    }

    ids
}

/// Save custom OAuth client IDs
pub fn save_oauth_client_ids(ids: &HashMap<String, String>) -> Result<(), String> {
    crate::storage::save_json("oauth_client_ids.json", ids)
}

/// Save a single OAuth client ID for a connector
pub fn save_oauth_client_id(connector_id: &str, client_id: &str) {
    let mut ids = load_oauth_client_ids();
    ids.insert(connector_id.to_string(), client_id.to_string());
    save_oauth_client_ids(&ids).ok();
}

// ─────────────────────────────────────────────
//  Proxy-based OAuth (for platforms requiring client_secret)
// ─────────────────────────────────────────────

const AUTH_PROXY_URL: &str = "https://connect.orbixcore.ai";

/// Platforms that need the auth proxy (future: Instagram, TikTok).
/// LinkedIn now uses direct OAuth with encrypted client_secret.
pub fn needs_proxy(_connector_id: &str) -> bool {
    false // All current platforms handled directly
}

/// Start an OAuth flow through the OrbixCore auth proxy.
/// The proxy holds the client_secret server-side. Tokens redirect back to localhost.
pub async fn start_proxy_oauth_flow(connector_id: &str) -> Result<OAuthTokenResponse, String> {
    let state = generate_state();

    // Start local callback server to receive tokens
    let (tx, rx) = oneshot::channel::<HashMap<String, String>>();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));
    let tx_clone = Arc::clone(&tx);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await
        .map_err(|e| format!("Failed to bind callback server: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    // Spawn callback handler
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();

            // Parse query params from GET request
            let mut params = HashMap::new();
            if let Some(query_start) = request.find('?') {
                let query_end = request[query_start..].find(' ').unwrap_or(request.len() - query_start);
                let query = &request[query_start + 1..query_start + query_end];
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        params.insert(
                            urlencoding::decode(k).unwrap_or_default().to_string(),
                            urlencoding::decode(v).unwrap_or_default().to_string(),
                        );
                    }
                }
            }

            let html = if params.contains_key("access_token") {
                "<html><body style='font-family:system-ui;text-align:center;padding:60px;background:#0e1117;color:#e0d0f0'><h2>Connected!</h2><p>You can close this tab and return to OrbixCore.</p></body></html>"
            } else {
                "<html><body style='font-family:system-ui;text-align:center;padding:60px;background:#0e1117;color:#f87171'><h2>Connection Failed</h2><p>Please try again from OrbixCore.</p></body></html>"
            };

            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}", html.len(), html);
            stream.write_all(response.as_bytes()).await.ok();

            if let Some(tx) = tx_clone.lock().await.take() {
                tx.send(params).ok();
            }
        }
    });

    // Open browser to auth proxy
    let redirect = format!("http://127.0.0.1:{}/callback", port);
    let proxy_url = format!("{}/auth/{}?redirect={}&state={}", AUTH_PROXY_URL, connector_id, urlencoding::encode(&redirect), state);

    open::that(&proxy_url).map_err(|e| format!("Failed to open browser: {}", e))?;

    // Wait for callback (5 minute timeout)
    let params = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        rx,
    ).await
        .map_err(|_| "Login timed out (5 minutes). Please try again.".to_string())?
        .map_err(|_| "Callback cancelled".to_string())?;

    // Check for errors
    if let Some(error) = params.get("error") {
        let desc = params.get("error_description").map(|s| s.as_str()).unwrap_or("Unknown error");
        return Err(format!("{}: {}", error, desc));
    }

    let access_token = params.get("access_token")
        .ok_or("No access token received from proxy")?
        .clone();

    Ok(OAuthTokenResponse {
        access_token,
        refresh_token: params.get("refresh_token").cloned(),
        expires_in: params.get("expires_in").and_then(|s| s.parse().ok()),
        token_type: "Bearer".to_string(),
        scope: None,
    })
}

// ─────────────────────────────────────────────
//  OAuth2 Authorization Code Flow with PKCE
// ─────────────────────────────────────────────

/// Start the full OAuth login flow:
/// 1. Start a local HTTP server to receive the callback
/// 2. Open the browser to the authorization URL
/// 3. Wait for the callback with the auth code
/// 4. Exchange the auth code for tokens
/// 5. Return the tokens
pub async fn start_oauth_flow(
    connector_id: &str,
) -> Result<OAuthTokenResponse, String> {
    let mut config = get_oauth_config(connector_id)
        .ok_or_else(|| format!("OAuth not supported for connector: {}", connector_id))?;

    // Load custom client IDs and secrets if configured
    let custom_ids = load_oauth_client_ids();
    if let Some(custom_id) = custom_ids.get(connector_id) {
        if !custom_id.is_empty() {
            config.client_id = custom_id.clone();
        }
    }
    // Load custom client secret (e.g., for LinkedIn)
    let secret_key = format!("{}_secret", connector_id);
    if let Some(custom_secret) = custom_ids.get(&secret_key) {
        if !custom_secret.is_empty() {
            config.client_secret = Some(custom_secret.clone());
        }
    }

    // Check if client_secret is still a placeholder (for platforms that require it)
    if let Some(ref secret) = config.client_secret {
        if secret.starts_with("ORBIXCORE_") {
            return Err(format!(
                "SETUP_GUIDE:To connect {name}, I need your LinkedIn Developer App credentials.\n\n\
                1. Go to linkedin.com/developers → Create App\n\
                2. Add the product \"Share on LinkedIn\" and \"Sign In with LinkedIn using OpenID Connect\"\n\
                3. In Auth settings, add redirect URL: http://localhost\n\
                4. Copy the **Client ID** and **Client Secret**\n\n\
                Tell me both values and I'll save them.",
                name = connector_id
            ));
        }
    }

    // Check if client_id is still a placeholder
    if config.client_id.starts_with("ORBIXCORE_") {
        let guide = match connector_id {
            "google" => "To connect Google (mail + calendar), you need a Google Cloud OAuth client ID.\n\n1. Go to console.cloud.google.com/apis/credentials\n2. Enable the Gmail API and the Google Calendar API for your project (APIs & Services -> Library). Without these, sending mail and creating events will fail even with a valid client ID.\n3. Create an OAuth 2.0 Client ID of type \"Desktop app\".\n4. Orbix uses a loopback redirect (http://127.0.0.1:<port>/callback); Desktop-app clients allow this automatically.\n5. Copy the Client ID and paste it below.",
            "github" => "To connect GitHub, you need a GitHub OAuth App.\n\n1. Go to github.com/settings/developers\n2. Create a new OAuth App\n3. Set the callback URL to http://localhost\n4. Copy the Client ID and paste it below.\n\nOr use a Personal Access Token instead (Settings → Developer settings → Tokens).",
            "slack" => "To connect Slack, you need a Slack App.\n\n1. Go to api.slack.com/apps\n2. Create a new app for your workspace\n3. Add OAuth scopes: channels:read, chat:write, users:read\n4. Copy the Client ID and paste it below.",
            _ => "OAuth is not pre-configured for this service. You'll need to register an app with the provider and enter your Client ID.",
        };
        return Err(format!("SETUP_GUIDE:{}", guide));
    }

    // Generate PKCE values
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();

    // Start local callback server
    let (tx, rx) = oneshot::channel::<(String, String)>();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));

    let expected_state = state.clone();
    let tx_clone = Arc::clone(&tx);

    // Bind to a fixed port if the provider requires it (e.g. LinkedIn needs
    // the redirect_uri to exactly match what's registered). Otherwise random.
    let bind_addr = if config.redirect_port > 0 {
        format!("127.0.0.1:{}", config.redirect_port)
    } else {
        "127.0.0.1:0".to_string()
    };
    let listener = tokio::net::TcpListener::bind(&bind_addr).await
        .map_err(|e| format!("Failed to start callback server on {}: {}", bind_addr, e))?;
    let port = listener.local_addr()
        .map_err(|e| format!("Failed to get port: {}", e))?
        .port();

    // Spawn the callback server
    let server_handle = tokio::spawn(async move {
        // Accept one connection with timeout
        let accept_result = tokio::time::timeout(
            std::time::Duration::from_secs(300), // 5 minute timeout
            listener.accept()
        ).await;

        let (mut stream, _) = match accept_result {
            Ok(Ok(s)) => s,
            _ => return,
        };

        let mut buf = vec![0u8; 4096];
        let n = match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
            Ok(n) => n,
            Err(_) => return,
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the GET request for code and state
        if let Some(query_start) = request.find('?') {
            let query_end = request[query_start..].find(' ').unwrap_or(request.len() - query_start);
            let query = &request[query_start + 1..query_start + query_end];
            let params: HashMap<String, String> = query.split('&')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    Some((
                        parts.next()?.to_string(),
                        urlencoding::decode(parts.next().unwrap_or("")).unwrap_or_default().to_string(),
                    ))
                })
                .collect();

            // Send success HTML response
            let html = r#"<!DOCTYPE html><html><head><style>
                body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #0a0a0a; color: #e0e0e0; }
                .box { text-align: center; padding: 40px; border-radius: 16px; background: #1a1a1a; border: 1px solid #333; }
                h1 { color: #7c5aed; margin-bottom: 8px; }
                p { color: #999; }
            </style></head><body><div class="box">
                <h1>Connected!</h1>
                <p>You can close this tab and return to OrbixCore.</p>
            </div></body></html>"#;

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(), html
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;

            // Validate state parameter
            if let Some(returned_state) = params.get("state") {
                if returned_state == &expected_state {
                    if let Some(code) = params.get("code") {
                        if let Some(tx) = tx_clone.lock().await.take() {
                            let _ = tx.send((code.clone(), String::new()));
                        }
                        return;
                    }
                }
            }

            // Check for error
            if let Some(error) = params.get("error") {
                if let Some(tx) = tx_clone.lock().await.take() {
                    let desc = params.get("error_description").cloned().unwrap_or_default();
                    let _ = tx.send((String::new(), format!("{}: {}", error, desc)));
                }
            }
        }
    });

    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

    // Build authorization URL
    let scope_str = config.scopes.join(" ");
    let auth_url = format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256&prompt=select_account",
        config.auth_url,
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&scope_str),
        urlencoding::encode(&state),
        urlencoding::encode(&code_challenge),
    );

    // Open system browser
    open::that(&auth_url).map_err(|e| format!("Failed to open browser: {}", e))?;

    // Wait for callback (with 5 minute timeout)
    let (code, error) = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        rx,
    ).await
        .map_err(|_| "Login timed out. Please try again.".to_string())?
        .map_err(|_| "Login was cancelled.".to_string())?;

    server_handle.abort();

    if !error.is_empty() {
        return Err(format!("OAuth error: {}", error));
    }
    if code.is_empty() {
        return Err("No authorization code received.".to_string());
    }

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let mut form = HashMap::new();
    form.insert("grant_type", "authorization_code".to_string());
    form.insert("client_id", config.client_id.clone());
    form.insert("code", code);
    form.insert("redirect_uri", redirect_uri);
    // PKCE for public clients, client_secret for confidential clients (LinkedIn)
    if let Some(ref secret) = config.client_secret {
        form.insert("client_secret", secret.clone());
    } else {
        form.insert("code_verifier", code_verifier);
    }

    let resp = client.post(&config.token_url)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed (HTTP {}): {}", status, body));
    }

    let token_resp: OAuthTokenResponse = resp.json().await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    Ok(token_resp)
}

/// Fetch user info using the access token to get display name and email
pub async fn fetch_user_info(connector_id: &str, access_token: &str) -> (Option<String>, Option<String>) {
    let client = reqwest::Client::new();

    match connector_id {
        "microsoft" => {
            if let Ok(resp) = client.get("https://graph.microsoft.com/v1.0/me")
                .bearer_auth(access_token)
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let name = json["displayName"].as_str().map(|s| s.to_string());
                    let email = json["mail"].as_str()
                        .or_else(|| json["userPrincipalName"].as_str())
                        .map(|s| s.to_string());
                    return (name, email);
                }
            }
        }
        "google" => {
            if let Ok(resp) = client.get("https://www.googleapis.com/oauth2/v3/userinfo")
                .bearer_auth(access_token)
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let name = json["name"].as_str().map(|s| s.to_string());
                    let email = json["email"].as_str().map(|s| s.to_string());
                    return (name, email);
                }
            }
        }
        "github" => {
            if let Ok(resp) = client.get("https://api.github.com/user")
                .bearer_auth(access_token)
                .header("User-Agent", "OrbixCore")
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let name = json["name"].as_str()
                        .or_else(|| json["login"].as_str())
                        .map(|s| s.to_string());
                    let email = json["email"].as_str().map(|s| s.to_string());
                    return (name, email);
                }
            }
        }
        "slack" => {
            if let Ok(resp) = client.get("https://slack.com/api/auth.test")
                .bearer_auth(access_token)
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let name = json["user"].as_str().map(|s| s.to_string());
                    let team = json["team"].as_str().map(|s| s.to_string());
                    return (name, team);
                }
            }
        }
        "linkedin" => {
            if let Ok(resp) = client.get("https://api.linkedin.com/v2/userinfo")
                .bearer_auth(access_token)
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let name = json["name"].as_str()
                        .or_else(|| json["given_name"].as_str())
                        .map(|s| s.to_string());
                    let email = json["email"].as_str().map(|s| s.to_string());
                    return (name, email);
                }
            }
        }
        _ => {}
    }

    (None, None)
}

// ─────────────────────────────────────────────
//  Device Code Flow - simple login without redirects
// ─────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    #[serde(default)]
    pub message: String,
}

/// Start a device code flow for Microsoft or Google.
/// Returns the device code response (user_code + verification URL).
/// Caller must then poll with `poll_device_code` until the user completes login.
pub async fn start_device_code_flow(
    connector_id: &str,
) -> Result<DeviceCodeResponse, String> {
    let config = get_oauth_config(connector_id)
        .ok_or_else(|| format!("OAuth not supported for: {}", connector_id))?;

    // Load custom client IDs
    let custom_ids = load_oauth_client_ids();
    let client_id = custom_ids.get(connector_id)
        .filter(|id| !id.is_empty())
        .cloned()
        .unwrap_or(config.client_id);

    if client_id.starts_with("ORBIXCORE_") {
        // Tell the user where the config file lives on the platform they're on.
        // The actual path comes from `dirs::config_dir()` at runtime; this is
        // just the user-facing hint string.
        #[cfg(target_os = "macos")]
        let config_hint = "~/Library/Application Support/ai.orbixcore.app/oauth_client_ids.json";
        #[cfg(target_os = "windows")]
        let config_hint = "%APPDATA%\\ai.orbixcore.app\\oauth_client_ids.json";
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let config_hint = "~/.config/ai.orbixcore.app/oauth_client_ids.json";

        return Err(format!(
            "No OAuth client ID configured for {}. Save it to {} as {{\"{}\":\"YOUR_CLIENT_ID\"}}",
            connector_id, config_hint, connector_id
        ));
    }

    let client = reqwest::Client::new();
    let scope_str = config.scopes.join(" ");

    let device_code_url = match connector_id {
        "microsoft" => "https://login.microsoftonline.com/common/oauth2/v2.0/devicecode".to_string(),
        "google" => "https://oauth2.googleapis.com/device/code".to_string(),
        _ => return Err(format!("Device code flow not supported for {}", connector_id)),
    };

    let resp = client.post(&device_code_url)
        .form(&[
            ("client_id", client_id.as_str()),
            ("scope", scope_str.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("Device code request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Device code request failed: {}", body));
    }

    let dcr: DeviceCodeResponse = resp.json().await
        .map_err(|e| format!("Failed to parse device code response: {}", e))?;

    Ok(dcr)
}

/// Poll for token completion after user enters the device code.
/// Returns tokens once the user completes sign-in.
pub async fn poll_device_code(
    connector_id: &str,
    device_code: &str,
) -> Result<OAuthTokenResponse, String> {
    let config = get_oauth_config(connector_id)
        .ok_or_else(|| format!("OAuth not supported for: {}", connector_id))?;

    let custom_ids = load_oauth_client_ids();
    let client_id = custom_ids.get(connector_id)
        .filter(|id| !id.is_empty())
        .cloned()
        .unwrap_or(config.client_id);

    let client = reqwest::Client::new();

    let grant_type = match connector_id {
        "microsoft" => "urn:ietf:params:oauth:grant-type:device_code",
        "google" => "urn:ietf:params:oauth:grant-type:device_code",
        _ => return Err("Unsupported".to_string()),
    };

    // Poll every 5 seconds for up to 5 minutes
    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let resp = client.post(&config.token_url)
            .form(&[
                ("grant_type", grant_type),
                ("client_id", client_id.as_str()),
                ("device_code", device_code),
            ])
            .send()
            .await
            .map_err(|e| format!("Poll failed: {}", e))?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();

        if status.is_success() {
            let token_resp: OAuthTokenResponse = serde_json::from_value(body)
                .map_err(|e| format!("Failed to parse token: {}", e))?;
            return Ok(token_resp);
        }

        // Check if still pending
        let error = body["error"].as_str().unwrap_or("");
        match error {
            "authorization_pending" | "slow_down" => continue,
            "expired_token" => return Err("Login timed out. Please try again.".to_string()),
            "access_denied" => return Err("Login was denied.".to_string()),
            _ => return Err(format!("Login failed: {}", body)),
        }
    }

    Err("Login timed out after 5 minutes.".to_string())
}
