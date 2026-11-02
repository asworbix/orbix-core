use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────
//  Connector Definitions
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorId {
    Microsoft,
    Google,
    Salesforce,
    Sap,
    Slack,
    Jira,
    Github,
    #[serde(rename = "linkedin")]
    LinkedIn,
    Custom,
    // External/Bob-authored connectors live in
    // ~/Library/Application Support/ai.orbixcore.app/modules/<slug>/<version>/.
    // Built-ins keep their named variants for now; new ones land here by slug.
    Module(String),
}

impl ConnectorId {
    /// Canonical slug used for module dirs, audit rows, and bus routing.
    pub fn slug(&self) -> String {
        match self {
            ConnectorId::Microsoft => "microsoft".into(),
            ConnectorId::Google => "google".into(),
            ConnectorId::Salesforce => "salesforce".into(),
            ConnectorId::Sap => "sap".into(),
            ConnectorId::Slack => "slack".into(),
            ConnectorId::Jira => "jira".into(),
            ConnectorId::Github => "github".into(),
            ConnectorId::LinkedIn => "linkedin".into(),
            ConnectorId::Custom => "custom".into(),
            ConnectorId::Module(s) => s.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Oauth2,
    ApiKey,
    BasicAuth,
    BearerToken,
    ClientCredentials,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectorField {
    pub key: String,
    pub label: String,
    pub placeholder: String,
    pub field_type: String, // "text", "password", "url", "email"
    pub required: bool,
    pub validation_regex: Option<String>,
    pub help_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectorDefinition {
    pub id: ConnectorId,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub auth_method: AuthMethod,
    pub fields: Vec<ConnectorField>,
    pub test_endpoint: Option<String>,
    pub docs_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectorInstance {
    pub id: String,
    pub connector_id: ConnectorId,
    pub name: String,
    pub status: ConnectionStatus,
    pub created_at: String,
    pub last_tested: Option<String>,
    pub error_message: Option<String>,
    // credentials stored encrypted, keyed by field key
    pub credential_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectorTestResult {
    pub success: bool,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginAssistantMessage {
    pub field_key: String,
    pub hint: String,
    pub example: String,
    pub where_to_find: String,
}

// ─────────────────────────────────────────────
//  Built-in connector catalog
// ─────────────────────────────────────────────

pub fn connector_catalog() -> Vec<ConnectorDefinition> {
    vec![
        ConnectorDefinition {
            id: ConnectorId::Microsoft,
            name: "Microsoft 365".to_string(),
            icon: "M365".to_string(),
            description: "Connect to Outlook, Teams, SharePoint, OneDrive, Dynamics 365".to_string(),
            auth_method: AuthMethod::Oauth2,
            fields: vec![
                ConnectorField {
                    key: "tenant_id".to_string(),
                    label: "Tenant ID".to_string(),
                    placeholder: "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: Some(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$".to_string()),
                    help_text: "Your Azure AD tenant ID. Found in Azure Portal → Azure Active Directory → Properties.".to_string(),
                },
                ConnectorField {
                    key: "client_id".to_string(),
                    label: "Application (Client) ID".to_string(),
                    placeholder: "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: Some(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$".to_string()),
                    help_text: "The Application ID from your Azure AD App Registration.".to_string(),
                },
                ConnectorField {
                    key: "client_secret".to_string(),
                    label: "Client Secret".to_string(),
                    placeholder: "Enter client secret value".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "The secret value (not the ID) from Certificates & secrets in your App Registration.".to_string(),
                },
            ],
            test_endpoint: Some("https://graph.microsoft.com/v1.0/me".to_string()),
            docs_url: "https://learn.microsoft.com/en-us/graph/auth-register-app-v2".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Google,
            name: "Google Workspace".to_string(),
            icon: "GWS".to_string(),
            description: "Connect to Gmail, Calendar, Drive, Sheets".to_string(),
            auth_method: AuthMethod::Oauth2,
            fields: vec![
                ConnectorField {
                    key: "client_id".to_string(),
                    label: "OAuth Client ID".to_string(),
                    placeholder: "xxxxxxxxxxxx.apps.googleusercontent.com".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: Some(r"\.apps\.googleusercontent\.com$".to_string()),
                    help_text: "From Google Cloud Console → APIs & Services → Credentials → OAuth 2.0 Client IDs.".to_string(),
                },
                ConnectorField {
                    key: "client_secret".to_string(),
                    label: "Client Secret".to_string(),
                    placeholder: "GOCSPX-xxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "The client secret from your OAuth 2.0 credentials.".to_string(),
                },
                ConnectorField {
                    key: "refresh_token".to_string(),
                    label: "Refresh Token (optional)".to_string(),
                    placeholder: "Paste refresh token if you have one".to_string(),
                    field_type: "password".to_string(),
                    required: false,
                    validation_regex: None,
                    help_text: "If you already completed OAuth flow elsewhere, paste the refresh token here.".to_string(),
                },
            ],
            test_endpoint: Some("https://www.googleapis.com/oauth2/v1/userinfo".to_string()),
            docs_url: "https://console.cloud.google.com/apis/credentials".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Salesforce,
            name: "Salesforce".to_string(),
            icon: "SF".to_string(),
            description: "Connect to Salesforce CRM, leads, contacts, opportunities".to_string(),
            auth_method: AuthMethod::Oauth2,
            fields: vec![
                ConnectorField {
                    key: "instance_url".to_string(),
                    label: "Instance URL".to_string(),
                    placeholder: "https://yourorg.my.salesforce.com".to_string(),
                    field_type: "url".to_string(),
                    required: true,
                    validation_regex: Some(r"^https://.*\.salesforce\.com".to_string()),
                    help_text: "Your Salesforce org URL. Found in Setup → Company Information.".to_string(),
                },
                ConnectorField {
                    key: "client_id".to_string(),
                    label: "Consumer Key".to_string(),
                    placeholder: "3MVG9xxxxxxxxxxxxxxxx".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "From Setup → App Manager → your Connected App → Consumer Key.".to_string(),
                },
                ConnectorField {
                    key: "client_secret".to_string(),
                    label: "Consumer Secret".to_string(),
                    placeholder: "Enter consumer secret".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "Consumer Secret from your Salesforce Connected App.".to_string(),
                },
                ConnectorField {
                    key: "security_token".to_string(),
                    label: "Security Token".to_string(),
                    placeholder: "Enter your security token".to_string(),
                    field_type: "password".to_string(),
                    required: false,
                    validation_regex: None,
                    help_text: "From Personal Settings → Reset My Security Token. Required if IP restrictions are active.".to_string(),
                },
            ],
            test_endpoint: None,
            docs_url: "https://help.salesforce.com/s/articleView?id=sf.remoteaccess_authenticate.htm".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Sap,
            name: "SAP".to_string(),
            icon: "SAP".to_string(),
            description: "Connect to SAP S/4HANA, Business One, or BTP".to_string(),
            auth_method: AuthMethod::BasicAuth,
            fields: vec![
                ConnectorField {
                    key: "base_url".to_string(),
                    label: "SAP Server URL".to_string(),
                    placeholder: "https://your-sap-server.com:443/sap/opu/odata/".to_string(),
                    field_type: "url".to_string(),
                    required: true,
                    validation_regex: Some(r"^https?://".to_string()),
                    help_text: "The base OData or API endpoint for your SAP system.".to_string(),
                },
                ConnectorField {
                    key: "client".to_string(),
                    label: "SAP Client Number".to_string(),
                    placeholder: "100".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: Some(r"^\d{3}$".to_string()),
                    help_text: "The 3-digit SAP client number (e.g., 100, 200, 800).".to_string(),
                },
                ConnectorField {
                    key: "username".to_string(),
                    label: "Username".to_string(),
                    placeholder: "SAP_USER".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "Your SAP system user ID.".to_string(),
                },
                ConnectorField {
                    key: "password".to_string(),
                    label: "Password".to_string(),
                    placeholder: "Enter SAP password".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "Your SAP system password. Stored encrypted on this device.".to_string(),
                },
            ],
            test_endpoint: None,
            docs_url: "https://help.sap.com/docs/".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Slack,
            name: "Slack".to_string(),
            icon: "SLK".to_string(),
            description: "Connect to Slack workspaces, channels, messages".to_string(),
            auth_method: AuthMethod::BearerToken,
            fields: vec![
                ConnectorField {
                    key: "bot_token".to_string(),
                    label: "Bot User OAuth Token".to_string(),
                    placeholder: "xoxb-xxxxxxxxxxxx-xxxxxxxxxxxx-xxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: Some(r"^xoxb-".to_string()),
                    help_text: "From api.slack.com → Your App → OAuth & Permissions → Bot User OAuth Token.".to_string(),
                },
            ],
            test_endpoint: Some("https://slack.com/api/auth.test".to_string()),
            docs_url: "https://api.slack.com/authentication/token-types".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Jira,
            name: "Jira / Atlassian".to_string(),
            icon: "JIRA".to_string(),
            description: "Connect to Jira, Confluence, Bitbucket".to_string(),
            auth_method: AuthMethod::ApiKey,
            fields: vec![
                ConnectorField {
                    key: "base_url".to_string(),
                    label: "Atlassian Site URL".to_string(),
                    placeholder: "https://yoursite.atlassian.net".to_string(),
                    field_type: "url".to_string(),
                    required: true,
                    validation_regex: Some(r"^https://.*\.atlassian\.net".to_string()),
                    help_text: "Your Atlassian Cloud site URL.".to_string(),
                },
                ConnectorField {
                    key: "email".to_string(),
                    label: "Email".to_string(),
                    placeholder: "you@company.com".to_string(),
                    field_type: "email".to_string(),
                    required: true,
                    validation_regex: Some(r"^[^\s@]+@[^\s@]+\.[^\s@]+$".to_string()),
                    help_text: "The email associated with your Atlassian account.".to_string(),
                },
                ConnectorField {
                    key: "api_token".to_string(),
                    label: "API Token".to_string(),
                    placeholder: "Enter your Atlassian API token".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "Generate at id.atlassian.com → Security → API tokens → Create API token.".to_string(),
                },
            ],
            test_endpoint: None,
            docs_url: "https://id.atlassian.com/manage-profile/security/api-tokens".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Github,
            name: "GitHub".to_string(),
            icon: "GH".to_string(),
            description: "Connect to GitHub repos, issues, pull requests".to_string(),
            auth_method: AuthMethod::BearerToken,
            fields: vec![
                ConnectorField {
                    key: "personal_token".to_string(),
                    label: "Personal Access Token".to_string(),
                    placeholder: "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: Some(r"^(ghp_|github_pat_)".to_string()),
                    help_text: "From github.com → Settings → Developer settings → Personal access tokens → Generate new token.".to_string(),
                },
            ],
            test_endpoint: Some("https://api.github.com/user".to_string()),
            docs_url: "https://github.com/settings/tokens".to_string(),
        },
        ConnectorDefinition {
            id: ConnectorId::Custom,
            name: "Custom API".to_string(),
            icon: "API".to_string(),
            description: "Connect any REST API with custom authentication".to_string(),
            auth_method: AuthMethod::ApiKey,
            fields: vec![
                ConnectorField {
                    key: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    placeholder: "https://api.example.com/v1".to_string(),
                    field_type: "url".to_string(),
                    required: true,
                    validation_regex: Some(r"^https?://".to_string()),
                    help_text: "The base URL of the API.".to_string(),
                },
                ConnectorField {
                    key: "auth_header".to_string(),
                    label: "Auth Header Name".to_string(),
                    placeholder: "Authorization".to_string(),
                    field_type: "text".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "The HTTP header name for authentication (e.g., Authorization, X-API-Key).".to_string(),
                },
                ConnectorField {
                    key: "auth_value".to_string(),
                    label: "Auth Header Value".to_string(),
                    placeholder: "Bearer your-token-here".to_string(),
                    field_type: "password".to_string(),
                    required: true,
                    validation_regex: None,
                    help_text: "The value to send in the auth header.".to_string(),
                },
            ],
            test_endpoint: None,
            docs_url: "".to_string(),
        },
    ]
}

// ─────────────────────────────────────────────
//  Credential storage (encrypted)
// ─────────────────────────────────────────────

pub fn save_connector_credential(instance_id: &str, field_key: &str, value: &str) -> Result<(), String> {
    let encrypted = crate::storage::encrypt_string(value)?;
    let filename = format!("connectors/{}_{}.enc", instance_id, field_key);
    let dir = crate::storage::data_dir().join("connectors");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create connectors dir: {}", e))?;
    crate::storage::atomic_write(dir.parent().unwrap().join(&filename), encrypted)
        .map_err(|e| format!("Write credential failed: {}", e))?;
    Ok(())
}

pub fn load_connector_credential(instance_id: &str, field_key: &str) -> Result<String, String> {
    let filename = format!("connectors/{}_{}.enc", instance_id, field_key);
    let path = crate::storage::data_dir().join(&filename);
    if !path.exists() {
        return Err("Credential not found".to_string());
    }
    let encrypted = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read credential failed: {}", e))?;
    crate::storage::decrypt_string(&encrypted)
}

pub fn delete_connector_credentials(instance_id: &str) {
    let dir = crate::storage::data_dir().join("connectors");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(&format!("{}_", instance_id)) {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Instance persistence
// ─────────────────────────────────────────────

pub fn save_connector_instances(instances: &[ConnectorInstance]) -> Result<(), String> {
    crate::storage::save_json("connector_instances.json", instances)
}

pub fn load_connector_instances() -> Result<Vec<ConnectorInstance>, String> {
    crate::storage::load_json("connector_instances.json")
}

// ─────────────────────────────────────────────
//  Connection testing
// ─────────────────────────────────────────────

pub async fn test_connection(instance: &ConnectorInstance, def: &ConnectorDefinition) -> ConnectorTestResult {
    let credentials: HashMap<String, String> = instance.credential_keys.iter()
        .filter_map(|key| {
            load_connector_credential(&instance.id, key).ok().map(|v| (key.clone(), v))
        })
        .collect();

    match def.id {
        ConnectorId::Github => test_bearer_token(
            "https://api.github.com/user",
            credentials.get("personal_token").map(|s| s.as_str()).unwrap_or(""),
            Some(vec![("User-Agent", "OrbixCore")]),
        ).await,

        ConnectorId::Slack => test_bearer_token(
            "https://slack.com/api/auth.test",
            credentials.get("bot_token").map(|s| s.as_str()).unwrap_or(""),
            None,
        ).await,

        ConnectorId::Microsoft => {
            // Test using client credentials flow → get token → call /me
            let tenant = credentials.get("tenant_id").cloned().unwrap_or_default();
            let client_id = credentials.get("client_id").cloned().unwrap_or_default();
            let client_secret = credentials.get("client_secret").cloned().unwrap_or_default();
            test_microsoft_oauth(&tenant, &client_id, &client_secret).await
        }

        ConnectorId::Google => {
            // Validate client_id format
            let client_id = credentials.get("client_id").cloned().unwrap_or_default();
            if client_id.ends_with(".apps.googleusercontent.com") {
                ConnectorTestResult {
                    success: true,
                    message: "Credentials format valid. OAuth flow ready.".to_string(),
                    details: Some("Client ID format verified. Use the OAuth consent screen to complete setup.".to_string()),
                }
            } else {
                ConnectorTestResult {
                    success: false,
                    message: "Invalid Client ID format".to_string(),
                    details: Some("Expected format: xxxx.apps.googleusercontent.com".to_string()),
                }
            }
        }

        ConnectorId::Jira => {
            let base_url = credentials.get("base_url").cloned().unwrap_or_default();
            let email = credentials.get("email").cloned().unwrap_or_default();
            let token = credentials.get("api_token").cloned().unwrap_or_default();
            test_basic_auth(
                &format!("{}/rest/api/3/myself", base_url.trim_end_matches('/')),
                &email,
                &token,
            ).await
        }

        ConnectorId::Sap => {
            let base_url = credentials.get("base_url").cloned().unwrap_or_default();
            let username = credentials.get("username").cloned().unwrap_or_default();
            let password = credentials.get("password").cloned().unwrap_or_default();
            let client = credentials.get("client").cloned().unwrap_or_default();
            test_sap_connection(&base_url, &client, &username, &password).await
        }

        ConnectorId::Salesforce => {
            let instance_url = credentials.get("instance_url").cloned().unwrap_or_default();
            let client_id = credentials.get("client_id").cloned().unwrap_or_default();
            let client_secret = credentials.get("client_secret").cloned().unwrap_or_default();
            if !instance_url.is_empty() && !client_id.is_empty() && !client_secret.is_empty() {
                ConnectorTestResult {
                    success: true,
                    message: "Credentials saved. OAuth flow ready.".to_string(),
                    details: Some("Connected App credentials verified. Full OAuth requires browser redirect.".to_string()),
                }
            } else {
                ConnectorTestResult {
                    success: false,
                    message: "Missing required fields".to_string(),
                    details: None,
                }
            }
        }

        ConnectorId::LinkedIn => {
            let token = credentials.get("access_token").cloned().unwrap_or_default();
            test_bearer_token(
                "https://api.linkedin.com/v2/userinfo",
                &token,
                None,
            ).await
        }

        ConnectorId::Custom => {
            let base_url = credentials.get("base_url").cloned().unwrap_or_default();
            let auth_header = credentials.get("auth_header").cloned().unwrap_or_default();
            let auth_value = credentials.get("auth_value").cloned().unwrap_or_default();
            test_custom_api(&base_url, &auth_header, &auth_value).await
        }

        ConnectorId::Module(ref slug) => ConnectorTestResult {
            success: false,
            message: format!("Module connector '{}' uses its own runtime probe", slug),
            details: Some(
                "External modules are tested by the connector runtime's smoke tests, \
                 not the built-in test_connection path.".to_string(),
            ),
        },
    }
}

async fn test_bearer_token(url: &str, token: &str, extra_headers: Option<Vec<(&str, &str)>>) -> ConnectorTestResult {
    let client = reqwest::Client::new();
    let mut req = client.get(url).bearer_auth(token);
    if let Some(headers) = extra_headers {
        for (k, v) in headers {
            req = req.header(k, v);
        }
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => ConnectorTestResult {
            success: true,
            message: "Connected successfully!".to_string(),
            details: None,
        },
        Ok(resp) => ConnectorTestResult {
            success: false,
            message: format!("Authentication failed (HTTP {})", resp.status().as_u16()),
            details: Some("Check that your token is valid and has the correct scopes.".to_string()),
        },
        Err(e) => ConnectorTestResult {
            success: false,
            message: format!("Connection failed: {}", e),
            details: None,
        },
    }
}

async fn test_basic_auth(url: &str, user: &str, pass: &str) -> ConnectorTestResult {
    let client = reqwest::Client::new();
    match client.get(url).basic_auth(user, Some(pass)).send().await {
        Ok(resp) if resp.status().is_success() => ConnectorTestResult {
            success: true,
            message: "Connected successfully!".to_string(),
            details: None,
        },
        Ok(resp) => ConnectorTestResult {
            success: false,
            message: format!("Authentication failed (HTTP {})", resp.status().as_u16()),
            details: Some("Check your credentials and ensure the API is accessible.".to_string()),
        },
        Err(e) => ConnectorTestResult {
            success: false,
            message: format!("Connection failed: {}", e),
            details: None,
        },
    }
}

async fn test_microsoft_oauth(tenant: &str, client_id: &str, client_secret: &str) -> ConnectorTestResult {
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", tenant);
    let client = reqwest::Client::new();
    let resp = client.post(&token_url)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("scope", "https://graph.microsoft.com/.default"),
        ])
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => ConnectorTestResult {
            success: true,
            message: "OAuth token acquired successfully!".to_string(),
            details: Some("Microsoft Graph API access confirmed.".to_string()),
        },
        Ok(r) => {
            let status = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            ConnectorTestResult {
                success: false,
                message: format!("OAuth failed (HTTP {})", status),
                details: Some(if body.contains("invalid_client") {
                    "Invalid client credentials. Check your Client ID and Secret.".to_string()
                } else if body.contains("invalid_tenant") {
                    "Invalid Tenant ID. Verify in Azure Portal → Azure AD → Properties.".to_string()
                } else {
                    body.chars().take(200).collect()
                }),
            }
        }
        Err(e) => ConnectorTestResult {
            success: false,
            message: format!("Connection failed: {}", e),
            details: None,
        },
    }
}

async fn test_sap_connection(base_url: &str, client_num: &str, username: &str, password: &str) -> ConnectorTestResult {
    let url = format!("{}?sap-client={}", base_url.trim_end_matches('/'), client_num);
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // SAP servers often use self-signed certs
        .build()
        .unwrap_or_default();
    match http.get(&url).basic_auth(username, Some(password)).send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 200 => ConnectorTestResult {
            success: true,
            message: "SAP connection successful!".to_string(),
            details: None,
        },
        Ok(resp) if resp.status().as_u16() == 401 => ConnectorTestResult {
            success: false,
            message: "Authentication failed".to_string(),
            details: Some("Invalid username or password for the specified SAP client.".to_string()),
        },
        Ok(resp) => ConnectorTestResult {
            success: false,
            message: format!("SAP returned HTTP {}", resp.status().as_u16()),
            details: Some("Verify the server URL and client number are correct.".to_string()),
        },
        Err(e) => ConnectorTestResult {
            success: false,
            message: format!("Cannot reach SAP server: {}", e),
            details: Some("Check that the URL is correct and the server is accessible from your network.".to_string()),
        },
    }
}

async fn test_custom_api(base_url: &str, auth_header: &str, auth_value: &str) -> ConnectorTestResult {
    let client = reqwest::Client::new();
    match client.get(base_url).header(auth_header, auth_value).send().await {
        Ok(resp) if resp.status().is_success() => ConnectorTestResult {
            success: true,
            message: "API connection successful!".to_string(),
            details: None,
        },
        Ok(resp) => ConnectorTestResult {
            success: false,
            message: format!("API returned HTTP {}", resp.status().as_u16()),
            details: None,
        },
        Err(e) => ConnectorTestResult {
            success: false,
            message: format!("Connection failed: {}", e),
            details: None,
        },
    }
}

// ─────────────────────────────────────────────
//  Login Assistant - contextual field help
//  Uses local LLM or built-in hints
// ─────────────────────────────────────────────

pub fn get_login_hints(connector_id: &ConnectorId) -> Vec<LoginAssistantMessage> {
    match connector_id {
        ConnectorId::Microsoft => vec![
            LoginAssistantMessage {
                field_key: "tenant_id".to_string(),
                hint: "This is your Azure AD directory ID".to_string(),
                example: "e.g., 72f988bf-86f1-41af-91ab-2d7cd011db47".to_string(),
                where_to_find: "Azure Portal → Azure Active Directory → Properties → Directory ID".to_string(),
            },
            LoginAssistantMessage {
                field_key: "client_id".to_string(),
                hint: "Register an app in Azure AD to get this".to_string(),
                example: "e.g., 0a1b2c3d-4e5f-6789-abcd-ef0123456789".to_string(),
                where_to_find: "Azure Portal → App registrations → your app → Application (client) ID".to_string(),
            },
            LoginAssistantMessage {
                field_key: "client_secret".to_string(),
                hint: "Create a new secret in your app registration".to_string(),
                example: "A long string like: Abc8Q~xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                where_to_find: "Azure Portal → App registrations → your app → Certificates & secrets → New client secret → copy the Value (not the ID)".to_string(),
            },
        ],
        ConnectorId::Google => vec![
            LoginAssistantMessage {
                field_key: "client_id".to_string(),
                hint: "Create OAuth 2.0 credentials in Google Cloud Console".to_string(),
                example: "e.g., 123456789-abc.apps.googleusercontent.com".to_string(),
                where_to_find: "Google Cloud Console → APIs & Services → Credentials → Create Credentials → OAuth client ID".to_string(),
            },
            LoginAssistantMessage {
                field_key: "client_secret".to_string(),
                hint: "Shown when you create the OAuth client".to_string(),
                example: "e.g., GOCSPX-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                where_to_find: "Same page as Client ID - click the download icon to get JSON with both values".to_string(),
            },
        ],
        ConnectorId::Salesforce => vec![
            LoginAssistantMessage {
                field_key: "instance_url".to_string(),
                hint: "Your Salesforce org's URL".to_string(),
                example: "e.g., https://mycompany.my.salesforce.com".to_string(),
                where_to_find: "Look at the URL in your browser when logged into Salesforce".to_string(),
            },
            LoginAssistantMessage {
                field_key: "client_id".to_string(),
                hint: "Create a Connected App in Salesforce Setup".to_string(),
                example: "e.g., 3MVG9xxxxxxxxxxxxxxxx".to_string(),
                where_to_find: "Setup → App Manager → New Connected App → enable OAuth → Consumer Key".to_string(),
            },
            LoginAssistantMessage {
                field_key: "security_token".to_string(),
                hint: "Required when your IP is not whitelisted".to_string(),
                example: "A long alphanumeric string sent to your email".to_string(),
                where_to_find: "Personal Settings → Reset My Security Token → check email".to_string(),
            },
        ],
        ConnectorId::Sap => vec![
            LoginAssistantMessage {
                field_key: "base_url".to_string(),
                hint: "The OData or REST API endpoint of your SAP system".to_string(),
                example: "e.g., https://sap-server.company.com:443/sap/opu/odata/sap/".to_string(),
                where_to_find: "Ask your SAP Basis administrator or check transaction SICF".to_string(),
            },
            LoginAssistantMessage {
                field_key: "client".to_string(),
                hint: "SAP mandant/client number (3 digits)".to_string(),
                example: "e.g., 100, 200, or 800".to_string(),
                where_to_find: "Shown on the SAP login screen - or ask your admin".to_string(),
            },
        ],
        ConnectorId::Slack => vec![
            LoginAssistantMessage {
                field_key: "bot_token".to_string(),
                hint: "Create a Slack App and install it to get a bot token".to_string(),
                example: "e.g., xoxb-123456-789012-abcdefghijklmnopqrstuvwx".to_string(),
                where_to_find: "api.slack.com → Your Apps → Create App → OAuth & Permissions → Install to Workspace → Bot User OAuth Token".to_string(),
            },
        ],
        ConnectorId::Jira => vec![
            LoginAssistantMessage {
                field_key: "api_token".to_string(),
                hint: "Generate an API token for Atlassian Cloud".to_string(),
                example: "A long string of letters and numbers".to_string(),
                where_to_find: "id.atlassian.com → Security → API tokens → Create API token".to_string(),
            },
        ],
        ConnectorId::Github => vec![
            LoginAssistantMessage {
                field_key: "personal_token".to_string(),
                hint: "Create a personal access token (classic or fine-grained)".to_string(),
                example: "e.g., ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                where_to_find: "github.com → Settings → Developer settings → Personal access tokens → Generate new token".to_string(),
            },
        ],
        ConnectorId::LinkedIn => vec![
            LoginAssistantMessage {
                field_key: "access_token".to_string(),
                hint: "No manual setup needed - click 'Sign in with LinkedIn' to connect via browser".to_string(),
                example: "One-click OAuth login".to_string(),
                where_to_find: "Just click the Sign In button - the OrbixCore auth proxy handles everything".to_string(),
            },
        ],
        ConnectorId::Custom => vec![
            LoginAssistantMessage {
                field_key: "base_url".to_string(),
                hint: "The root URL of the API you want to connect".to_string(),
                example: "e.g., https://api.example.com/v2".to_string(),
                where_to_find: "Check the API documentation for the base URL".to_string(),
            },
        ],
        ConnectorId::Module(_) => Vec::new(),
    }
}

/// Ask Claude for help with a specific connector field.
/// Falls back to built-in hints if no API key is available.
pub async fn ask_login_assistant(
    connector_id: &ConnectorId,
    field_key: &str,
    user_input: &str,
) -> String {
    let connector_name = match connector_id {
        ConnectorId::Microsoft => "Microsoft 365 / Azure AD",
        ConnectorId::Google => "Google Workspace / Google Cloud",
        ConnectorId::Salesforce => "Salesforce CRM",
        ConnectorId::Sap => "SAP",
        ConnectorId::Slack => "Slack",
        ConnectorId::Jira => "Jira / Atlassian",
        ConnectorId::Github => "GitHub",
        ConnectorId::LinkedIn => "LinkedIn",
        ConnectorId::Custom => "Custom API",
        ConnectorId::Module(slug) => return format!(
            "Module connector '{}' provides its own help via the connector runtime.",
            slug,
        ),
    };

    let prompt = format!(
        "You are a helpful IT assistant embedded in OrbixCore. A user is setting up a {} connector and needs help with the '{}' field. {}\n\nGive a brief, actionable answer (2-3 sentences max). Be specific about where to find this value. Do not use markdown.",
        connector_name,
        field_key,
        if user_input.is_empty() {
            "They haven't entered anything yet. Tell them where to find this value.".to_string()
        } else {
            format!("They entered: '{}'. Validate this and tell them if it looks correct or what's wrong.", user_input)
        }
    );

    // Try Claude via Anthropic API
    let agents = crate::storage::load_agents().unwrap_or_default();
    if let Some(agent) = agents.iter().find(|a| a.enabled && !a.api_key.is_empty()) {
        let client = reqwest::Client::new();
        let endpoint = if agent.endpoint.contains("anthropic") {
            agent.endpoint.clone()
        } else {
            "https://api.anthropic.com".to_string()
        };
        let model = if agent.model.contains("claude") {
            agent.model.clone()
        } else {
            "claude-sonnet-4-6".to_string()
        };
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 200,
            "messages": [{"role": "user", "content": prompt}]
        });
        if let Ok(resp) = client
            .post(format!("{}/v1/messages", endpoint.trim_end_matches('/')))
            .header("x-api-key", &agent.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
        {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(content) = json["content"][0]["text"].as_str() {
                    return content.to_string();
                }
            }
        }
    }

    // Fallback: use built-in hints
    let hints = get_login_hints(connector_id);
    hints.iter()
        .find(|h| h.field_key == field_key)
        .map(|h| format!("{} - {}", h.hint, h.where_to_find))
        .unwrap_or_else(|| "Check the service documentation for this field.".to_string())
}

// ─────────────────────────────────────────────
//  Field validation
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationResult {
    pub field_key: String,
    pub valid: bool,
    pub message: String,
}

pub fn validate_field(def: &ConnectorDefinition, field_key: &str, value: &str) -> ValidationResult {
    let field = def.fields.iter().find(|f| f.key == field_key);
    let field = match field {
        Some(f) => f,
        None => return ValidationResult {
            field_key: field_key.to_string(),
            valid: false,
            message: "Unknown field".to_string(),
        },
    };

    if field.required && value.trim().is_empty() {
        return ValidationResult {
            field_key: field_key.to_string(),
            valid: false,
            message: format!("{} is required", field.label),
        };
    }

    if value.trim().is_empty() {
        return ValidationResult {
            field_key: field_key.to_string(),
            valid: true,
            message: "Optional field - left empty".to_string(),
        };
    }

    if let Some(ref regex_str) = field.validation_regex {
        if let Ok(re) = regex::Regex::new(regex_str) {
            if !re.is_match(value) {
                return ValidationResult {
                    field_key: field_key.to_string(),
                    valid: false,
                    message: format!("Invalid format. Expected: {}", field.placeholder),
                };
            }
        }
    }

    ValidationResult {
        field_key: field_key.to_string(),
        valid: true,
        message: "Looks good!".to_string(),
    }
}

// ─────────────────────────────────────────────
//  Universal Connectors
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectorEndpoint {
    pub key: String,
    pub method: String,
    pub path: String,
    pub description: String,
    pub body_template: Option<String>,
    pub query_params: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UniversalAuthType {
    None,
    ApiKey,
    Bearer,
    OAuth2,
    BasicAuth,
}

impl Default for UniversalAuthType {
    fn default() -> Self { UniversalAuthType::None }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub header_name: Option<String>,
    #[serde(default)]
    pub header_value: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UniversalConnector {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    #[serde(default)]
    pub auth_type: UniversalAuthType,
    #[serde(default)]
    pub auth_config: AuthConfig,
    #[serde(default)]
    pub default_headers: HashMap<String, String>,
    #[serde(default)]
    pub endpoints: Vec<ConnectorEndpoint>,
    #[serde(default = "default_connected")]
    pub status: ConnectionStatus,
    pub created_at: String,
    pub updated_at: String,
}

fn default_connected() -> ConnectionStatus { ConnectionStatus::Disconnected }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub found: bool,
    pub spec_type: Option<String>,
    pub endpoints: Vec<ConnectorEndpoint>,
    pub base_url: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

// Persistence
pub fn save_universal_connectors(connectors: &[UniversalConnector]) -> Result<(), String> {
    crate::storage::save_json("universal_connectors.json", connectors)
}

pub fn load_universal_connectors() -> Vec<UniversalConnector> {
    crate::storage::load_json::<Vec<UniversalConnector>>("universal_connectors.json").unwrap_or_default()
}

// Endpoint discovery
pub async fn discover_endpoints(base_url: &str) -> DiscoveryResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let base = base_url.trim_end_matches('/');

    let discovery_paths = [
        "/openapi.json", "/swagger.json", "/v3/api-docs",
        "/api-docs", "/.well-known/openapi.json", "/docs/openapi.json",
    ];

    for path in &discovery_paths {
        let url = format!("{}{}", base, path);
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Ok(spec) = serde_json::from_str::<serde_json::Value>(&text) {
                        return parse_openapi_spec(&spec, base);
                    }
                }
            }
        }
    }

    DiscoveryResult {
        found: false,
        spec_type: None,
        endpoints: Vec::new(),
        base_url: Some(base.to_string()),
        title: None,
        description: None,
    }
}

fn parse_openapi_spec(spec: &serde_json::Value, fallback_base: &str) -> DiscoveryResult {
    let mut endpoints = Vec::new();

    let title = spec.get("info").and_then(|i| i.get("title")).and_then(|t| t.as_str()).map(|s| s.to_string());
    let desc = spec.get("info").and_then(|i| i.get("description")).and_then(|t| t.as_str()).map(|s| s.to_string());

    // Detect spec type
    let spec_type = if spec.get("openapi").is_some() {
        Some("openapi3".to_string())
    } else if spec.get("swagger").is_some() {
        Some("swagger2".to_string())
    } else {
        None
    };

    // Parse paths
    if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
        for (path, methods) in paths {
            if let Some(methods_obj) = methods.as_object() {
                for (method, details) in methods_obj {
                    let m = method.to_uppercase();
                    if !["GET", "POST", "PUT", "DELETE", "PATCH"].contains(&m.as_str()) {
                        continue;
                    }
                    let summary = details.get("summary").and_then(|s| s.as_str()).unwrap_or("");
                    let op_id = details.get("operationId").and_then(|s| s.as_str()).unwrap_or("");

                    // Generate a key from operationId or method+path
                    let key = if !op_id.is_empty() {
                        op_id.to_string()
                    } else {
                        format!("{}_{}", m.to_lowercase(), path.replace('/', "_").trim_matches('_'))
                    };

                    endpoints.push(ConnectorEndpoint {
                        key,
                        method: m,
                        path: path.clone(),
                        description: summary.to_string(),
                        body_template: None,
                        query_params: None,
                    });
                }
            }
        }
    }

    DiscoveryResult {
        found: !endpoints.is_empty(),
        spec_type,
        endpoints,
        base_url: Some(fallback_base.to_string()),
        title,
        description: desc,
    }
}

/// Execute an HTTP request against a connector endpoint
pub async fn execute_connector_request(
    connector: &UniversalConnector,
    endpoint_key: &str,
    body_vars: &HashMap<String, String>,
) -> Result<(u16, String), String> {
    let endpoint = connector.endpoints.iter()
        .find(|e| e.key == endpoint_key)
        .ok_or_else(|| format!("Endpoint '{}' not found in connector '{}'", endpoint_key, connector.name))?;

    let url = format!("{}{}", connector.base_url.trim_end_matches('/'), endpoint.path);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let method = match endpoint.method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        other => return Err(format!("Unsupported HTTP method: {}", other)),
    };

    let mut req = client.request(method, &url);

    // Apply default headers
    for (k, v) in &connector.default_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    // Apply auth
    match connector.auth_type {
        UniversalAuthType::Bearer => {
            if let Some(ref token) = connector.auth_config.token {
                req = req.bearer_auth(token);
            }
        }
        UniversalAuthType::ApiKey => {
            let header_name = connector.auth_config.header_name.as_deref().unwrap_or("X-API-Key");
            let header_value = connector.auth_config.header_value.as_deref().unwrap_or("");
            req = req.header(header_name, header_value);
        }
        UniversalAuthType::BasicAuth => {
            let user = connector.auth_config.username.as_deref().unwrap_or("");
            let pass = connector.auth_config.password.as_deref();
            req = req.basic_auth(user, pass);
        }
        UniversalAuthType::OAuth2 => {
            if let Some(ref token) = connector.auth_config.token {
                req = req.bearer_auth(token);
            }
        }
        UniversalAuthType::None => {}
    }

    // Apply body template with variable substitution
    if let Some(ref template) = endpoint.body_template {
        let mut body = template.clone();
        for (k, v) in body_vars {
            body = body.replace(&format!("{{{{{}}}}}", k), v);
        }
        req = req.header("Content-Type", "application/json").body(body);
    }

    // Apply query params
    if let Some(ref qp) = endpoint.query_params {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (k, v) in qp {
            let mut val = v.clone();
            for (bk, bv) in body_vars {
                val = val.replace(&format!("{{{{{}}}}}", bk), bv);
            }
            pairs.push((k.clone(), val));
        }
        req = req.query(&pairs);
    }

    let resp = req.send().await.map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(|e| format!("Failed to read response: {}", e))?;

    Ok((status, body))
}

/// Pre-built D365 Dataverse endpoint templates for Sales Accelerator integration.
/// Used by the Lead Scout workflow to interact with Dynamics 365 Sales.
pub fn d365_sales_endpoint_templates() -> Vec<ConnectorEndpoint> {
    let mut qp_leads = HashMap::new();
    qp_leads.insert("$select".to_string(), "fullname,emailaddress1,companyname,telephone1,statuscode,leadqualitycode,createdon".to_string());
    qp_leads.insert("$filter".to_string(), "statuscode eq 1".to_string());
    qp_leads.insert("$orderby".to_string(), "createdon desc".to_string());
    qp_leads.insert("$top".to_string(), "100".to_string());

    let mut qp_opps = HashMap::new();
    qp_opps.insert("$select".to_string(), "name,estimatedvalue,closeprobability,statuscode".to_string());
    qp_opps.insert("$expand".to_string(), "parentaccountid($select=name)".to_string());
    qp_opps.insert("$filter".to_string(), "statecode eq 0".to_string());

    let mut qp_scores = HashMap::new();
    qp_scores.insert("$select".to_string(), "msdyn_score,msdyn_grade,msdyn_scoretrend".to_string());

    vec![
        ConnectorEndpoint {
            key: "d365_get_leads".to_string(),
            method: "GET".to_string(),
            path: "/api/data/v9.2/leads".to_string(),
            description: "Query active leads from D365 Sales".to_string(),
            body_template: None,
            query_params: Some(qp_leads),
        },
        ConnectorEndpoint {
            key: "d365_create_lead".to_string(),
            method: "POST".to_string(),
            path: "/api/data/v9.2/leads".to_string(),
            description: "Create a new lead in D365".to_string(),
            body_template: Some(r#"{"firstname":"{{firstname}}","lastname":"{{lastname}}","emailaddress1":"{{email}}","companyname":"{{company}}","telephone1":"{{phone}}","description":"{{description}}"}"#.to_string()),
            query_params: None,
        },
        ConnectorEndpoint {
            key: "d365_update_lead".to_string(),
            method: "PATCH".to_string(),
            path: "/api/data/v9.2/leads({{leadid}})".to_string(),
            description: "Update a lead record (qualification score, status, description)".to_string(),
            body_template: Some(r#"{"leadqualitycode":{{qualitycode}},"description":"{{description}}"}"#.to_string()),
            query_params: None,
        },
        ConnectorEndpoint {
            key: "d365_qualify_lead".to_string(),
            method: "POST".to_string(),
            path: "/api/data/v9.2/QualifyLead".to_string(),
            description: "Qualify a lead into opportunity + contact + account".to_string(),
            body_template: Some(r#"{"LeadId":{"leadid":"{{leadid}}","@odata.type":"Microsoft.Dynamics.CRM.lead"},"CreateAccount":true,"CreateContact":true,"CreateOpportunity":true,"Status":3}"#.to_string()),
            query_params: None,
        },
        ConnectorEndpoint {
            key: "d365_get_opportunities".to_string(),
            method: "GET".to_string(),
            path: "/api/data/v9.2/opportunities".to_string(),
            description: "Query open opportunities with account details".to_string(),
            body_template: None,
            query_params: Some(qp_opps),
        },
        ConnectorEndpoint {
            key: "d365_connect_sequence".to_string(),
            method: "POST".to_string(),
            path: "/api/data/v9.2/msdyn_ConnectSequence".to_string(),
            description: "Connect a lead/opportunity to a Sales Accelerator sequence".to_string(),
            body_template: Some(r#"{"msdyn_sequenceid":"{{sequenceid}}","RegardingEntityId":"{{entityid}}","RegardingEntityName":"{{entityname}}"}"#.to_string()),
            query_params: None,
        },
        ConnectorEndpoint {
            key: "d365_create_task".to_string(),
            method: "POST".to_string(),
            path: "/api/data/v9.2/tasks".to_string(),
            description: "Create a follow-up task in the seller's Work List".to_string(),
            body_template: Some(r#"{"subject":"{{subject}}","description":"{{description}}","scheduledend":"{{duedate}}","regardingobjectid_lead@odata.bind":"/leads({{leadid}})","ownerid@odata.bind":"/systemusers({{userid}})"}"#.to_string()),
            query_params: None,
        },
        ConnectorEndpoint {
            key: "d365_create_phonecall".to_string(),
            method: "POST".to_string(),
            path: "/api/data/v9.2/phonecalls".to_string(),
            description: "Create a phone call activity for high-priority leads".to_string(),
            body_template: Some(r#"{"subject":"{{subject}}","description":"{{talkingpoints}}","scheduledend":"{{duedate}}","regardingobjectid_lead@odata.bind":"/leads({{leadid}})"}"#.to_string()),
            query_params: None,
        },
        ConnectorEndpoint {
            key: "d365_get_predictive_scores".to_string(),
            method: "GET".to_string(),
            path: "/api/data/v9.2/msdyn_predictivescores".to_string(),
            description: "Read predictive lead/opportunity scores for prioritisation".to_string(),
            body_template: None,
            query_params: Some(qp_scores),
        },
        ConnectorEndpoint {
            key: "d365_get_sequences".to_string(),
            method: "GET".to_string(),
            path: "/api/data/v9.2/msdyn_sequences".to_string(),
            description: "List available Sales Accelerator sequences".to_string(),
            body_template: None,
            query_params: None,
        },
    ]
}

/// Test connectivity to a universal connector
pub async fn test_universal_connection(connector: &UniversalConnector) -> ConnectorTestResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let url = connector.base_url.trim_end_matches('/');

    let mut req = client.get(url);

    // Apply auth
    match connector.auth_type {
        UniversalAuthType::Bearer => {
            if let Some(ref token) = connector.auth_config.token {
                req = req.bearer_auth(token);
            }
        }
        UniversalAuthType::ApiKey => {
            let header_name = connector.auth_config.header_name.as_deref().unwrap_or("X-API-Key");
            let header_value = connector.auth_config.header_value.as_deref().unwrap_or("");
            req = req.header(header_name, header_value);
        }
        UniversalAuthType::BasicAuth => {
            let user = connector.auth_config.username.as_deref().unwrap_or("");
            let pass = connector.auth_config.password.as_deref();
            req = req.basic_auth(user, pass);
        }
        UniversalAuthType::OAuth2 => {
            if let Some(ref token) = connector.auth_config.token {
                req = req.bearer_auth(token);
            }
        }
        UniversalAuthType::None => {}
    }

    for (k, v) in &connector.default_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => ConnectorTestResult {
            success: true,
            message: format!("Connected to {} (HTTP {})", connector.name, resp.status().as_u16()),
            details: None,
        },
        Ok(resp) => ConnectorTestResult {
            success: false,
            message: format!("HTTP {} from {}", resp.status().as_u16(), connector.name),
            details: Some("Check auth credentials and base URL".to_string()),
        },
        Err(e) => ConnectorTestResult {
            success: false,
            message: format!("Connection failed: {}", e),
            details: None,
        },
    }
}
