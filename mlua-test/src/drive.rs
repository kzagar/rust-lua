use crate::gmail::{get_valid_token, GmailState};
use crate::types::AppState;
use mlua::prelude::*;
use std::sync::{Arc, Mutex};
use std::fs;
use std::path::Path;
use chrono::DateTime;
use rusqlite::{params, OptionalExtension};

#[derive(Clone)]
pub struct DriveFile {
    pub id: Option<String>,
    pub name: String,
    pub mime_type: Option<String>,
    pub path: Option<String>,
    pub blob: Option<Vec<u8>>,
    pub email: Option<String>,
    pub state: Option<Arc<GmailState>>,
}

impl LuaUserData for DriveFile {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("mime_type", |_, this| Ok(this.mime_type.clone()));
        fields.add_field_method_get("path", |_, this| Ok(this.path.clone()));
        fields.add_field_method_get("id", |_, this| Ok(this.id.clone()));
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("mime", |_, this, m: String| {
            this.mime_type = Some(m);
            Ok(this.clone())
        });

        methods.add_method_mut("path", |_, this, p: String| {
            this.path = Some(p);
            this.blob = None;
            if this.mime_type.is_none() {
                this.mime_type = Some(detect_mime(&this.name));
            }
            Ok(this.clone())
        });

        methods.add_method_mut("blob", |_, this, b: LuaValue| {
            if let LuaValue::String(s) = b {
                this.blob = Some(s.as_bytes().to_vec());
                this.path = None;
                if this.mime_type.is_none() {
                    this.mime_type = Some(detect_mime(&this.name));
                }
                Ok(this.clone())
            } else {
                Err(LuaError::RuntimeError("expected string for blob".into()))
            }
        });

        methods.add_async_method("get_blob", |lua: Lua, this, ()| async move {
            let data = if let Some(ref b) = this.blob {
                b.clone()
            } else if let Some(ref p) = this.path {
                fs::read(p).map_err(|e| LuaError::RuntimeError(e.to_string()))?
            } else if let Some(ref id) = this.id {
                let state = this.state.as_ref().ok_or_else(|| LuaError::RuntimeError("File has no drive state to download".into()))?;
                let email = this.email.as_ref().ok_or_else(|| LuaError::RuntimeError("File has no email to download".into()))?;

                let token = get_valid_token(state.clone(), email).await?;
                let res = state.client.get(format!("https://www.googleapis.com/drive/v3/files/{}?alt=media", id))
                    .bearer_auth(&token)
                    .send()
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                if !res.status().is_success() {
                    let err_text = res.text().await.unwrap_or_default();
                    return Err(LuaError::RuntimeError(format!("Failed to download file content: {}", err_text)));
                }

                let content = res.bytes().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                content.to_vec()
            } else {
                return Err(LuaError::RuntimeError("No data in file object".into()));
            };

            Ok(lua.create_string(&data)?)
        });
    }
}

fn detect_mime(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match ext.to_lowercase().as_str() {
        "json" => "application/json".to_string(),
        "txt" => "text/plain".to_string(),
        "html" => "text/html".to_string(),
        "pdf" => "application/pdf".to_string(),
        "zip" => "application/zip".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

pub struct Drive {
    pub email: String,
    pub state: Arc<GmailState>,
}

impl LuaUserData for Drive {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("search", |lua: Lua, this, options: LuaTable| async move {
            let mut q_parts = Vec::new();
            if let Some(after) = options.get::<Option<i64>>("after")? {
                let dt = DateTime::from_timestamp(after, 0).unwrap_or_default();
                q_parts.push(format!("modifiedTime > '{}'", dt.to_rfc3339()));
            }
            if let Some(query) = options.get::<Option<String>>("q")? {
                q_parts.push(query);
            }

            let q = q_parts.join(" and ");

            let token = get_valid_token(this.state.clone(), &this.email).await?;
            let mut request = this.state.client.get("https://www.googleapis.com/drive/v3/files");
            if !q.is_empty() {
                request = request.query(&[("q", q.as_str())]);
            }

            let res = request
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let json: serde_json::Value = res.json().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let files_json = json.get("files").and_then(|f| f.as_array()).ok_or_else(|| LuaError::RuntimeError("Invalid response".into()))?;

            let results = lua.create_table()?;
            for (i, f) in files_json.iter().enumerate() {
                 let drive_file = DriveFile {
                     id: f.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                     name: f.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                     mime_type: f.get("mimeType").and_then(|v| v.as_str()).map(|s| s.to_string()),
                     path: None,
                     blob: None,
                     email: Some(this.email.clone()),
                     state: Some(this.state.clone()),
                 };
                 results.set(i + 1, drive_file)?;
            }
            Ok(results)
        });

        methods.add_async_method("get_id", |_, this, path: String| async move {
            resolve_path(this.state.clone(), &this.email, &path).await
        });

        methods.add_async_method("get_file", |_, this, id: String| async move {
            let token = get_valid_token(this.state.clone(), &this.email).await?;

            // Get metadata
            let res = this.state.client.get(format!("https://www.googleapis.com/drive/v3/files/{}", id))
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let metadata: serde_json::Value = res.json().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            // Get content
            let res = this.state.client.get(format!("https://www.googleapis.com/drive/v3/files/{}?alt=media", id))
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            if !res.status().is_success() {
                let err_text = res.text().await.unwrap_or_default();
                return Err(LuaError::RuntimeError(format!("Failed to get file content: {}", err_text)));
            }

            let content = res.bytes().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            Ok(DriveFile {
                id: Some(id),
                name: metadata.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                mime_type: metadata.get("mimeType").and_then(|v| v.as_str()).map(|s| s.to_string()),
                path: None,
                blob: Some(content.to_vec()),
                email: Some(this.email.clone()),
                state: Some(this.state.clone()),
            })
        });

        methods.add_async_method("get_folder", |lua: Lua, this, id_or_path: String| async move {
            let id = if id_or_path.starts_with('/') || id_or_path == "root" {
                resolve_path(this.state.clone(), &this.email, &id_or_path).await?
            } else {
                id_or_path
            };

            let token = get_valid_token(this.state.clone(), &this.email).await?;
            let q = format!("'{}' in parents and trashed = false", id);
            let res = this.state.client.get("https://www.googleapis.com/drive/v3/files")
                .query(&[("q", q.as_str()), ("fields", "files(id, name, modifiedTime)")])
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let json: serde_json::Value = res.json().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let files_json = json.get("files").and_then(|f| f.as_array()).ok_or_else(|| LuaError::RuntimeError("Invalid response".into()))?;

            let results = lua.create_table()?;
            for (i, f) in files_json.iter().enumerate() {
                let item = lua.create_table()?;
                item.set("id", f.get("id").and_then(|v| v.as_str()).unwrap_or_default())?;
                item.set("name", f.get("name").and_then(|v| v.as_str()).unwrap_or_default())?;
                item.set("modifiedTime", f.get("modifiedTime").and_then(|v| v.as_str()).unwrap_or_default())?;
                results.set(i + 1, item)?;
            }
            Ok(results)
        });

        methods.add_async_method("upload_file", |_, this, (folder_id_or_path, file_val): (String, LuaAnyUserData)| async move {
            let folder_id = if folder_id_or_path.starts_with('/') || folder_id_or_path == "root" {
                resolve_path(this.state.clone(), &this.email, &folder_id_or_path).await?
            } else {
                folder_id_or_path
            };

            let file = file_val.borrow::<DriveFile>()?;

            let token = get_valid_token(this.state.clone(), &this.email).await?;

            // Check if file exists in this folder
            let q = format!("name = '{}' and '{}' in parents and trashed = false", file.name.replace("'", "\\'"), folder_id);
            let res = this.state.client.get("https://www.googleapis.com/drive/v3/files")
                .query(&[("q", q.as_str()), ("fields", "files(id)")])
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let json: serde_json::Value = res.json().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let files = json.get("files").and_then(|f| f.as_array()).unwrap();

            let data = if let Some(ref b) = file.blob {
                b.clone()
            } else if let Some(ref p) = file.path {
                fs::read(p).map_err(|e| LuaError::RuntimeError(e.to_string()))?
            } else {
                return Err(LuaError::RuntimeError("File has no data".into()));
            };

            if !files.is_empty() {
                // Update
                let file_id = files[0].get("id").and_then(|v| v.as_str()).unwrap();
                let res = this.state.client.patch(format!("https://www.googleapis.com/upload/drive/v3/files/{}?uploadType=media", file_id))
                    .bearer_auth(&token)
                    .header("Content-Type", file.mime_type.as_deref().unwrap_or("application/octet-stream"))
                    .body(data)
                    .send()
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                Ok(res.status().is_success())
            } else {
                // Create
                let metadata = serde_json::json!({
                    "name": file.name,
                    "parents": [folder_id],
                    "mimeType": file.mime_type
                });

                let form = reqwest::multipart::Form::new()
                    .part("metadata", reqwest::multipart::Part::text(metadata.to_string())
                        .mime_str("application/json").unwrap())
                    .part("file", reqwest::multipart::Part::bytes(data)
                        .mime_str(file.mime_type.as_deref().unwrap_or("application/octet-stream")).unwrap());

                let res = this.state.client.post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
                    .bearer_auth(&token)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                Ok(res.status().is_success())
            }
        });

        methods.add_method("new_file", |_, this, name: String| {
            Ok(DriveFile {
                id: None,
                name,
                mime_type: None,
                path: None,
                blob: None,
                email: Some(this.email.clone()),
                state: Some(this.state.clone()),
            })
        });
    }
}

async fn resolve_path(state: Arc<GmailState>, email: &str, path: &str) -> LuaResult<String> {
    if path == "root" || path == "/" {
        return Ok("root".to_string());
    }
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut current_id = "root".to_string();

    for part in parts {
        let token = get_valid_token(state.clone(), email).await?;
        let query = format!("name = '{}' and '{}' in parents and trashed = false", part.replace("'", "\\'"), current_id);
        let res = state.client.get("https://www.googleapis.com/drive/v3/files")
            .query(&[("q", query.as_str()), ("fields", "files(id)")])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

        let json: serde_json::Value = res.json().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        let files = json.get("files").and_then(|f| f.as_array()).ok_or_else(|| LuaError::RuntimeError("Invalid response from Drive API".into()))?;

        if files.is_empty() {
            return Err(LuaError::RuntimeError(format!("Path not found: {}", path)));
        }
        if files.len() > 1 {
            return Err(LuaError::RuntimeError(format!("Ambiguous path: {}", path)));
        }
        current_id = files[0].get("id").and_then(|v| v.as_str()).ok_or_else(|| LuaError::RuntimeError("Missing ID in response".into()))?.to_string();
    }
    Ok(current_id)
}

pub fn register(lua: &Lua, _app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    let drive_mod = lua.create_table()?;
    let state_clone = _app_state.clone();

    drive_mod.set("login", lua.create_async_function(move |lua: Lua, email: String| {
        let state_clone = state_clone.clone();
        async move {
            let drive_state = {
                let state = state_clone.lock().unwrap();
                state.drive_state.clone()
            };

            let drive_state = match drive_state {
                Some(s) => s,
                None => return Err(LuaError::RuntimeError("Drive state not initialized".into())),
            };

            let scopes: Option<String> = {
                let db = drive_state.db_conn.lock().unwrap();
                db.query_row("SELECT scopes FROM google_tokens WHERE email = ?", params![email], |row| row.get(0))
                    .optional()
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            };

            let mut authorized = false;
            if let Some(s) = scopes {
                if s.contains("https://www.googleapis.com/auth/drive") {
                    authorized = true;
                }
            }

            let res = lua.create_table()?;
            if authorized {
                res.set("status", "authorized")?;
                res.set("drive", Drive { email, state: drive_state })?;
            } else {
                let mut auth_url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
                {
                    let mut query = auth_url.query_pairs_mut();
                    query.append_pair("client_id", &drive_state.config.client_id);
                    query.append_pair("redirect_uri", &drive_state.config.redirect_uri);
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

    drive_mod.set("new_file", lua.create_function(|_, name: String| {
        Ok(DriveFile {
            id: None,
            name,
            mime_type: None,
            path: None,
            blob: None,
            email: None,
            state: None,
        })
    })?)?;

    lua.globals().set("drive", drive_mod)?;
    Ok(())
}
