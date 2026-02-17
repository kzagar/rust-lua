use crate::types::AppState;
use base64::Engine;
use chrono::Utc;
use mlua::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct GmailConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

pub struct GmailState {
    pub config: GmailConfig,
    pub db_conn: Arc<Mutex<Connection>>,
    pub attachment_manager: Arc<AttachmentManager>,
}

pub struct AttachmentManager {
    pub dir: PathBuf,
    pub ref_counts: Mutex<HashMap<PathBuf, usize>>,
}

impl AttachmentManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            ref_counts: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_path(&self, attachment_id: &str, filename: &str) -> PathBuf {
        self.dir.join(format!("{}_{}", attachment_id, filename))
    }

    pub fn add_ref(&self, path: &Path) {
        let mut counts = self.ref_counts.lock().unwrap();
        *counts.entry(path.to_path_buf()).or_insert(0) += 1;
    }

    pub fn remove_ref(&self, path: &Path) {
        let mut counts = self.ref_counts.lock().unwrap();
        if let Some(count) = counts.get_mut(path) {
            *count -= 1;
            if *count == 0 {
                counts.remove(path);
                let _ = fs::remove_file(path);
            }
        }
    }
}

pub struct Mailbox {
    pub email: String,
    pub state: Arc<GmailState>,
}

impl LuaUserData for Mailbox {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "search",
            |lua: Lua, mailbox, options: LuaTable| async move {
                let mut query = String::new();
                if let Some(after) = options.get::<Option<i64>>("after")? {
                    query.push_str(&format!("after:{} ", after));
                }
                if let Some(q) = options.get::<Option<String>>("q")? {
                    query.push_str(&q);
                }

                let token = get_valid_token(mailbox.state.clone(), &mailbox.email).await?;

                let res = tokio::task::spawn_blocking(move || {
                    ureq::get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
                        .query("q", query.trim())
                        .set("Authorization", &format!("Bearer {}", token))
                        .call()
                })
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let json: serde_json::Value = res.into_json().map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let messages = lua.create_table()?;
                if let Some(msgs) = json.get("messages").and_then(|m| m.as_array()) {
                    for (i, msg) in msgs.iter().enumerate() {
                        let id = msg
                            .get("id")
                            .and_then(|v: &serde_json::Value| v.as_str())
                            .unwrap_or_default();
                        messages.set(i + 1, id)?;
                    }
                }
                Ok(messages)
            },
        );

        methods.add_async_method("get_message", |_, mailbox, id: String| async move {
            let token = get_valid_token(mailbox.state.clone(), &mailbox.email).await?;
            let url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}",
                id
            );

            let res = tokio::task::spawn_blocking(move || {
                ureq::get(&url)
                    .set("Authorization", &format!("Bearer {}", token))
                    .call()
            })
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let json: serde_json::Value = res.into_json().map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let message = Message {
                id: id.clone(),
                data: json,
                mailbox: mailbox.clone(),
                attachments: Mutex::new(Vec::new()),
            };

            Ok(message)
        });

        methods.add_async_method(
            "prepare_draft",
            |_, mailbox, draft_info: LuaTable| async move {
                let to = draft_info.get::<Option<String>>("to")?.unwrap_or_default();
                let cc = draft_info.get::<Option<String>>("cc")?.unwrap_or_default();
                let bcc = draft_info.get::<Option<String>>("bcc")?.unwrap_or_default();
                let subject = draft_info
                    .get::<Option<String>>("subject")?
                    .unwrap_or_default();
                let body = draft_info
                    .get::<Option<String>>("body")?
                    .unwrap_or_default();
                let attachments_lua = draft_info.get::<Option<LuaTable>>("attachments")?;

                let mut attachments = Vec::new();
                if let Some(att_table) = attachments_lua {
                    for pair in att_table.pairs::<String, String>() {
                        let (name, path) = pair?;
                        let data =
                            fs::read(path).map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                        attachments.push((name, data));
                    }
                }

                let mime = construct_mime(&to, &cc, &bcc, &subject, &body, attachments);
                let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mime);

                let token = get_valid_token(mailbox.state.clone(), &mailbox.email).await?;
                let res = tokio::task::spawn_blocking(move || {
                    ureq::post("https://gmail.googleapis.com/gmail/v1/users/me/drafts")
                        .set("Authorization", &format!("Bearer {}", token))
                        .send_json(serde_json::json!({
                            "message": {
                                "raw": raw
                            }
                        }))
                })
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let json: serde_json::Value = res.into_json().map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                Ok(json
                    .get("id")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .map(|s| s.to_string()))
            },
        );

        methods.add_async_method("send_draft", |_, mailbox, draft_id: String| async move {
            let token = get_valid_token(mailbox.state.clone(), &mailbox.email).await?;
            
            let res = tokio::task::spawn_blocking(move || {
                ureq::post("https://gmail.googleapis.com/gmail/v1/users/me/drafts/send")
                    .set("Authorization", &format!("Bearer {}", token))
                    .send_json(serde_json::json!({
                        "id": draft_id
                    }))
            })
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            Ok(res.status() >= 200 && res.status() < 300)
        });

        methods.add_async_method(
            "send_message",
            |_, mailbox, draft_info: LuaTable| async move {
                let to = draft_info.get::<Option<String>>("to")?.unwrap_or_default();
                let cc = draft_info.get::<Option<String>>("cc")?.unwrap_or_default();
                let bcc = draft_info.get::<Option<String>>("bcc")?.unwrap_or_default();
                let subject = draft_info
                    .get::<Option<String>>("subject")?
                    .unwrap_or_default();
                let body = draft_info
                    .get::<Option<String>>("body")?
                    .unwrap_or_default();
                let attachments_lua = draft_info.get::<Option<LuaTable>>("attachments")?;

                let mut attachments = Vec::new();
                if let Some(att_table) = attachments_lua {
                    for pair in att_table.pairs::<String, String>() {
                        let (name, path) = pair?;
                        let data =
                            fs::read(path).map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                        attachments.push((name, data));
                    }
                }

                let mime = construct_mime(&to, &cc, &bcc, &subject, &body, attachments);
                let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mime);

                let token = get_valid_token(mailbox.state.clone(), &mailbox.email).await?;

                let res = tokio::task::spawn_blocking(move || {
                    ureq::post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
                        .set("Authorization", &format!("Bearer {}", token))
                        .send_json(serde_json::json!({
                            "raw": raw
                        }))
                })
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                Ok(res.status() >= 200 && res.status() < 300)
            },
        );
    }
}

fn construct_mime(
    to: &str,
    cc: &str,
    bcc: &str,
    subject: &str,
    body: &str,
    attachments: Vec<(String, Vec<u8>)>,
) -> String {
    let boundary = format!("----={}", uuid::Uuid::new_v4());
    let mut mime = String::new();
    mime.push_str(&format!("To: {}\r\n", to));
    if !cc.is_empty() {
        mime.push_str(&format!("Cc: {}\r\n", cc));
    }
    if !bcc.is_empty() {
        mime.push_str(&format!("Bcc: {}\r\n", bcc));
    }
    mime.push_str(&format!("Subject: {}\r\n", subject));
    mime.push_str("MIME-Version: 1.0\r\n");

    if attachments.is_empty() {
        mime.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
        mime.push_str("Content-Transfer-Encoding: 7bit\r\n\r\n");
        mime.push_str(body);
    } else {
        mime.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"{}\"\r\n\r\n",
            boundary
        ));
        mime.push_str(&format!("--{}\r\n", boundary));
        mime.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
        mime.push_str("Content-Transfer-Encoding: 7bit\r\n\r\n");
        mime.push_str(body);
        mime.push_str("\r\n");

        for (filename, data) in attachments {
            mime.push_str(&format!("--{}\r\n", boundary));
            mime.push_str(&format!(
                "Content-Type: application/octet-stream; name=\"{}\"\r\n",
                filename
            ));
            mime.push_str("Content-Transfer-Encoding: base64\r\n");
            mime.push_str(&format!(
                "Content-Disposition: attachment; filename=\"{}\"\r\n\r\n",
                filename
            ));
            mime.push_str(&base64::engine::general_purpose::STANDARD.encode(data));
            mime.push_str("\r\n");
        }
        mime.push_str(&format!("--{}--\r\n", boundary));
    }

    mime
}

impl Clone for Mailbox {
    fn clone(&self) -> Self {
        Self {
            email: self.email.clone(),
            state: self.state.clone(),
        }
    }
}

pub struct Message {
    pub id: String,
    pub data: serde_json::Value,
    pub mailbox: Mailbox,
    pub attachments: Mutex<Vec<PathBuf>>,
}

impl LuaUserData for Message {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get_info", |lua: &Lua, this, ()| {
            let info = lua.create_table()?;
            info.set("id", this.id.clone())?;
            info.set(
                "snippet",
                this.data
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            )?;

            if let Some(payload) = this.data.get("payload") {
                if let Some(headers_arr) = payload
                    .get("headers")
                    .and_then(|h: &serde_json::Value| h.as_array())
                {
                    let h_table = lua.create_table()?;
                    for h in headers_arr {
                        let name = h
                            .get("name")
                            .and_then(|v: &serde_json::Value| v.as_str())
                            .unwrap_or("");
                        let value = h
                            .get("value")
                            .and_then(|v: &serde_json::Value| v.as_str())
                            .unwrap_or("");
                        h_table.set(name, value)?;
                    }
                    info.set("headers", h_table)?;
                }

                // Simplified body extraction (looking for text/plain or text/html)
                let mut body_text = String::new();
                let mut body_html = String::new();

                extract_body(payload, &mut body_text, &mut body_html);

                info.set("body_text", body_text)?;
                info.set("body_html", body_html)?;
            }

            Ok(info)
        });

        methods.add_async_method("download_attachments", |lua: Lua, this, ()| async move {
            let token = get_valid_token(this.mailbox.state.clone(), &this.mailbox.email).await?;
            let mut paths: Vec<PathBuf> = Vec::new();

            let mut attachments_info = Vec::new();
            find_attachments(&this.data, &mut attachments_info);

            for (filename, attachment_id) in attachments_info {
                let path = this
                    .mailbox
                    .state
                    .attachment_manager
                    .get_path(&attachment_id, &filename);

                if !path.exists() {
                    let url = format!(
                        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
                        this.id, attachment_id
                    );

                    let token_clone = token.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        ureq::get(&url)
                            .set("Authorization", &format!("Bearer {}", token_clone))
                            .call()
                    })
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                    let json: serde_json::Value = res.into_json().map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                    if let Some(data) = json
                        .get("data")
                        .and_then(|v: &serde_json::Value| v.as_str())
                    {
                        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .decode(data)
                            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                        fs::write(&path, decoded)
                            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                    }
                }

                this.mailbox.state.attachment_manager.add_ref(&path);
                paths.push(path.clone());
            }

            {
                let mut atts = this.attachments.lock().unwrap();
                atts.extend(paths.clone());
            }

            let lua_paths = lua.create_table()?;
            for (i, p) in paths.iter().enumerate() {
                lua_paths.set(i + 1, p.to_string_lossy().to_string())?;
            }
            Ok(lua_paths)
        });
    }
}

#[allow(clippy::collapsible_if)]
fn find_attachments(part: &serde_json::Value, attachments: &mut Vec<(String, String)>) {
    if let Some(attachment_id) = part
        .get("filename")
        .and_then(|v: &serde_json::Value| v.as_str())
        .filter(|s| !s.is_empty())
        .and_then(|filename| {
            part.get("body")
                .and_then(|b| b.get("attachmentId"))
                .and_then(|v: &serde_json::Value| v.as_str())
                .map(|id| (filename, id))
        })
    {
        attachments.push((attachment_id.0.to_string(), attachment_id.1.to_string()));
    }
    if let Some(parts) = part.get("parts").and_then(|p| p.as_array()) {
        for p in parts {
            find_attachments(p, attachments);
        }
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        let atts = self.attachments.lock().unwrap();
        for path in atts.iter() {
            self.mailbox.state.attachment_manager.remove_ref(path);
        }
    }
}

#[allow(clippy::collapsible_if)]
fn extract_body(part: &serde_json::Value, text: &mut String, html: &mut String) {
    if let Some(mime_type) = part
        .get("mimeType")
        .and_then(|v: &serde_json::Value| v.as_str())
    {
        if mime_type == "text/plain" {
            if let Some(decoded) = part
                .get("body")
                .and_then(|b| b.get("data"))
                .and_then(|d: &serde_json::Value| d.as_str())
                .and_then(|data| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(data)
                        .ok()
                })
            {
                *text = String::from_utf8_lossy(&decoded).to_string();
            }
        } else if mime_type == "text/html" {
            if let Some(decoded) = part
                .get("body")
                .and_then(|b| b.get("data"))
                .and_then(|d: &serde_json::Value| d.as_str())
                .and_then(|data| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(data)
                        .ok()
                })
            {
                *html = String::from_utf8_lossy(&decoded).to_string();
            }
        }
    }

    if let Some(parts) = part.get("parts").and_then(|p| p.as_array()) {
        for p in parts {
            extract_body(p, text, html);
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    _scope: Option<String>,
}

pub async fn get_valid_token(state: Arc<GmailState>, email: &str) -> LuaResult<String> {
    let (access_token, refresh_token, expires_at) = {
        let conn = state.db_conn.lock().unwrap();
        conn.query_row(
            "SELECT access_token, refresh_token, expires_at FROM google_tokens WHERE email = ?",
            params![email],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| LuaError::RuntimeError(e.to_string()))?
        .ok_or_else(|| LuaError::RuntimeError(format!("No token for {}", email)))?
    };

    let is_expired = expires_at
        .map(|exp| exp < Utc::now() + chrono::Duration::try_seconds(60).unwrap())
        .unwrap_or(false);

    if is_expired {
        if let Some(rf_token) = refresh_token {
            let client_id = state.config.client_id.clone();
            let client_secret = state.config.client_secret.clone();
            let res = tokio::task::spawn_blocking(move || {
                ureq::post("https://oauth2.googleapis.com/token")
                    .set("Content-Type", "application/x-www-form-urlencoded")
                    .send_string(&format!(
                        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
                        client_id, client_secret, rf_token
                    ))
            })
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .map_err(|e| LuaError::RuntimeError(format!("Failed to refresh token: {}", e)))?;

            let token_res: TokenResponse = res.into_json().map_err(|e| {
                LuaError::RuntimeError(format!("Failed to parse refresh response: {}", e))
            })?;

            let new_access_token = token_res.access_token;
            let new_refresh_token = token_res.refresh_token;
            let new_expires_at = token_res
                .expires_in
                .map(|s| chrono::Utc::now() + chrono::Duration::try_seconds(s).unwrap());

            let conn = state.db_conn.lock().unwrap();
            conn.execute(
                "UPDATE google_tokens SET access_token = ?, refresh_token = COALESCE(?, refresh_token), expires_at = ? WHERE email = ?",
                params![&new_access_token, &new_refresh_token, new_expires_at, email],
            )
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            return Ok(new_access_token);
        } else {
            return Err(LuaError::RuntimeError(
                "Token expired and no refresh token available".into(),
            ));
        }
    }

    Ok(access_token)
}

pub fn register(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    let gmail = lua.create_table()?;
    let state_clone = app_state.clone();

    gmail.set("login", lua.create_async_function(move |lua: Lua, email: String| {
        let state_clone = state_clone.clone();
        async move {
            let gmail_state = {
                let state = state_clone.lock().unwrap();
                state.gmail_state.clone()
            };

            let gmail_state = match gmail_state {
                Some(s) => s,
                None => return Err(LuaError::RuntimeError("Gmail not initialized".into())),
            };

            let row = {
                let conn = gmail_state.db_conn.lock().unwrap();
                conn.query_row(
                    "SELECT 1 FROM google_tokens WHERE email = ?",
                    params![&email],
                    |_| Ok(1),
                )
                .optional()
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            };

            let res = lua.create_table()?;
            if row.is_some() {
                res.set("status", "authorized")?;
                res.set("mailbox", Mailbox { email, state: gmail_state })?;
            } else {
                let mut auth_url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
                {
                    let mut query = auth_url.query_pairs_mut();
                    query.append_pair("client_id", &gmail_state.config.client_id);
                    query.append_pair("redirect_uri", &gmail_state.config.redirect_uri);
                    query.append_pair("response_type", "code");
                    query.append_pair("scope", "https://www.googleapis.com/auth/gmail.modify https://www.googleapis.com/auth/gmail.compose https://www.googleapis.com/auth/drive");
                    query.append_pair("access_type", "offline");
                    query.append_pair("prompt", "consent");
                    query.append_pair("state", &email);
                }

                res.set("status", "unauthorized")?;
                res.set("auth_url", auth_url.to_string())?;
            }
            Ok(res)
        }
    })?)?;

    lua.globals().set("gmail", gmail)?;
    Ok(())
}

pub async fn handle_callback(
    state: Arc<GmailState>,
    code: String,
    email: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = state.config.client_id.clone();
    let client_secret = state.config.client_secret.clone();
    let redirect_uri = state.config.redirect_uri.clone();
    let res = tokio::task::spawn_blocking(move || {
        ureq::post("https://oauth2.googleapis.com/token")
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&format!(
                "client_id={}&client_secret={}&code={}&grant_type=authorization_code&redirect_uri={}",
                client_id,
                client_secret,
                code,
                urlencoding::encode(&redirect_uri)
            ))
    }).await??;

    let token_res: TokenResponse = res.into_json()?;

    let access_token = token_res.access_token;
    let refresh_token = token_res.refresh_token;
    let expires_at = token_res
        .expires_in
        .map(|s| chrono::Utc::now() + chrono::Duration::try_seconds(s).unwrap());

    let conn = state.db_conn.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO google_tokens (email, access_token, refresh_token, expires_at) VALUES (?, ?, ?, ?)",
        params![email, access_token, refresh_token, expires_at],
    )?;

    Ok(())
}

pub async fn init_gmail_state() -> Result<Arc<GmailState>, Box<dyn std::error::Error>> {
    let secrets_path = Path::new(".secrets");
    if !secrets_path.exists() {
        return Err("No .secrets file found".into());
    }
    let content = fs::read_to_string(secrets_path)?;
    let mut lines = content.lines();
    let credentials_path = lines.next().ok_or("Empty .secrets file")?;
    let attachment_dir_str = lines.next();

    let cred_content = fs::read_to_string(credentials_path)?;
    let config_json: serde_json::Value = serde_json::from_str(&cred_content)?;

    let installed = config_json
        .get("installed")
        .or(config_json.get("web"))
        .ok_or("Invalid credentials JSON")?;
    let client_id = installed
        .get("client_id")
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or("Missing client_id")?;
    let client_secret = installed
        .get("client_secret")
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or("Missing client_secret")?;
    let redirect_uri = installed
        .get("redirect_uris")
        .and_then(|v: &serde_json::Value| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or("Missing redirect_uri")?;

    let config = GmailConfig {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        redirect_uri: redirect_uri.to_string(),
    };

    let db_conn = Connection::open("tokens.db")?;
    db_conn.execute("CREATE TABLE IF NOT EXISTS google_tokens (email TEXT PRIMARY KEY, access_token TEXT, refresh_token TEXT, expires_at DATETIME)", [])?;
    let db_conn = Arc::new(Mutex::new(db_conn));

    let attachment_dir = attachment_dir_str
        .map(|s| s.to_string())
        .or_else(|| std::env::var("GMAIL_ATTACHMENT_DIR").ok())
        .unwrap_or_else(|| "attachments".to_string());

    let attachment_dir = PathBuf::from(attachment_dir);
    if !attachment_dir.exists() {
        fs::create_dir_all(&attachment_dir)?;
    }

    Ok(Arc::new(GmailState {
        config,
        db_conn,
        attachment_manager: Arc::new(AttachmentManager::new(attachment_dir)),
    }))
}
