use crate::types::{AppState, EngineRequest};
use mlua::prelude::*;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

pub struct TelegramBotGuard(pub JoinHandle<()>);

impl Drop for TelegramBotGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn register(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    let telegram = lua.create_table()?;
    let state_clone = app_state.clone();

    telegram.set(
        "on_update",
        lua.create_function(move |lua, func: LuaFunction| {
            let mut state = state_clone.lock().unwrap();
            state.telegram_handler = Some(lua.create_registry_value(func)?);
            Ok(())
        })?,
    )?;

    telegram.set(
        "send_message",
        lua.create_async_function(|_, (chat_id, text): (LuaValue, String)| async move {
            let token = std::env::var("TELEGRAM_BOT_TOKEN")
                .map_err(|_| LuaError::RuntimeError("TELEGRAM_BOT_TOKEN not set".into()))?;
            let chat_id_val: JsonValue = match chat_id {
                LuaValue::String(s) => JsonValue::String(s.to_str()?.to_string()),
                LuaValue::Integer(i) => JsonValue::Number(i.into()),
                LuaValue::Number(n) => {
                    if n as i64 as f64 == n {
                        JsonValue::Number((n as i64).into())
                    } else {
                        JsonValue::Number(
                            serde_json::Number::from_f64(n)
                                .ok_or_else(|| LuaError::RuntimeError("Invalid number".into()))?,
                        )
                    }
                }
                _ => {
                    return Err(LuaError::RuntimeError(
                        "chat_id must be string or number".into(),
                    ));
                }
            };

            let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
            let body = serde_json::to_string(&serde_json::json!({
                "chat_id": chat_id_val,
                "text": text
            })).map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let res = tokio::task::spawn_blocking(move || {
                minreq::post(&url)
                    .with_header("Content-Type", "application/json")
                    .with_body(body)
                    .send()
            }).await.map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            if res.status_code < 200 || res.status_code >= 300 {
                let body = res.as_str().unwrap_or_default();
                return Ok((false, Some(format!("Telegram API error: {}", body))));
            }

            Ok((true, None))
        })?,
    )?;

    lua.globals().set("telegram", telegram)?;
    Ok(())
}

pub async fn start(
    app_state: Arc<Mutex<AppState>>,
    tx: Sender<EngineRequest>,
) -> Option<TelegramBotGuard> {
    let has_handler = {
        let state = app_state.lock().unwrap();
        state.telegram_handler.is_some()
    };

    if !has_handler {
        return None;
    }

    let token = match std::env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            eprintln!("TELEGRAM_BOT_TOKEN not set, skipping telegram bot start");
            return None;
        }
    };

    Some(TelegramBotGuard(tokio::spawn(async move {
        println!("Telegram bot long polling started.");
        let mut offset = 0;
        let url = format!("https://api.telegram.org/bot{}/getUpdates", token);

        loop {
            let current_url = format!("{}?offset={}&timeout=30", url, offset);
            let res = tokio::task::spawn_blocking(move || {
                minreq::get(current_url).send()
            }).await;

            match res {
                Ok(Ok(resp)) => {
                    if resp.status_code >= 200 && resp.status_code < 300 {
                        let json: JsonValue = match serde_json::from_str(resp.as_str().unwrap_or("{}")) {
                            Ok(j) => j,
                            Err(e) => {
                                eprintln!("Failed to parse telegram updates: {}", e);
                                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                continue;
                            }
                        };

                        if let Some(updates) = json.get("result").and_then(|r| r.as_array()) {
                            for update in updates {
                                if let Some(update_id) =
                                    update.get("update_id").and_then(|id| id.as_i64())
                                {
                                    offset = update_id + 1;
                                }
                                if tx
                                    .send(EngineRequest::TelegramUpdate(update.clone()))
                                    .await
                                    .is_err()
                                {
                                    return; // Channel closed, exit task
                                }
                            }
                        }
                    } else {
                        eprintln!("Telegram getUpdates failed with status: {}", resp.status_code);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
                _ => {
                    eprintln!("Telegram getUpdates request error");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    })))
}
