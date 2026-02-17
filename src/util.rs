use hmac::{Hmac, Mac};
use mlua::prelude::*;
use sha2::Sha256;
use std::env;
use std::fs;
use std::path::Path;

type HmacSha256 = Hmac<Sha256>;

pub fn load_secrets() {
    let mut files_to_check = Vec::new();

    // Check ~/.secrets
    if let Some(mut path) = dirs::home_dir() {
        path.push(".secrets");
        if path.exists() && path.is_file() {
            files_to_check.push(path);
        }
    }

    // Check .secrets in current directory (overrides home logic if we want, but usually we just load both.
    // Last one loaded wins if we overwrite, or first one wins if we don't overwrite.
    // Let's make CWD win (so load Home first, then CWD).
    // And let's PREFER env vars already set (so don't overwrite if set).
    let cwd_secrets = Path::new(".secrets");
    if cwd_secrets.exists() && cwd_secrets.is_file() {
        // Avoid duplication if CWD is home
        if !files_to_check.contains(&cwd_secrets.to_path_buf()) {
            files_to_check.push(cwd_secrets.to_path_buf());
        }
    }

    for path in files_to_check {
        println!("Loading secrets from {:?}", path);
        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                // Ignore comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let mut value = value.trim();

                    // Handle quotes
                    if (value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\''))
                    {
                        value = &value[1..value.len() - 1];
                    }

                    // Only set if not already set in the actual environment
                    if env::var(key).is_err() {
                        // Handle escaped newlines (common in private keys)
                        let val_string = value.replace("\\n", "\n");
                        // SAFETY: This is called at the beginning of main, before any other threads are spawned.
                        unsafe {
                            env::set_var(key, val_string);
                        }
                    }
                }
            }
        }
    }
}


pub fn register(lua: &Lua) -> LuaResult<()> {
    let logging = lua.create_table()?;
    logging.set(
        "debug",
        lua.create_function(|_, msg: String| {
            log::debug!("{}", msg);
            Ok(())
        })?,
    )?;
    logging.set(
        "info",
        lua.create_function(|_, msg: String| {
            log::info!("{}", msg);
            Ok(())
        })?,
    )?;
    logging.set(
        "warn",
        lua.create_function(|_, msg: String| {
            log::warn!("{}", msg);
            Ok(())
        })?,
    )?;
    logging.set(
        "error",
        lua.create_function(|_, msg: String| {
            log::error!("{}", msg);
            Ok(())
        })?,
    )?;
    logging.set(
        "fatal",
        lua.create_function(|_, msg: String| {
            // We use error level for fatal, but the logger could be improved to handle this.
            // For GCP, we might want CRITICAL.
            log::error!("[FATAL] {}", msg);
            Ok(())
        })?,
    )?;
    lua.globals().set("logging", logging)?;

    let crypto = lua.create_table()?;
    crypto.set(
        "hmac_sha256",
        lua.create_function(|_, (key, data): (String, String)| {
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            mac.update(data.as_bytes());
            let result = mac.finalize().into_bytes();
            Ok(hex::encode(result))
        })?,
    )?;
    lua.globals().set("crypto", crypto)?;

    let url = lua.create_table()?;
    url.set(
        "encode",
        lua.create_function(|_, s: String| Ok(urlencoding::encode(&s).to_string()))?,
    )?;

    url.set(
        "encode_query",
        lua.create_function(|_, params: LuaTable| {
            let mut pairs = Vec::new();
            for pair in params.pairs::<String, String>() {
                let (k, v) = pair?;
                pairs.push(format!(
                    "{}={}",
                    urlencoding::encode(&k),
                    urlencoding::encode(&v)
                ));
            }
            Ok(pairs.join("&"))
        })?,
    )?;
    lua.globals().set("url", url)?;

    // Helper to extract and flatten tasks from MultiValue (handles both functions and tables of functions)
    let extract_tasks = |tasks: LuaMultiValue| -> LuaResult<Vec<LuaFunction>> {
        let mut extracted = Vec::new();
        for task in tasks {
            match task {
                LuaValue::Function(f) => extracted.push(f),
                LuaValue::Table(t) => {
                    // Try to iterate as a sequence (ipairs style) first, 
                    // or just all values if it's not a strict sequence.
                    for value in t.sequence_values::<LuaFunction>() {
                        extracted.push(value?);
                    }
                }
                _ => {}
            }
        }
        Ok(extracted)
    };

    lua.globals().set(
        "parallel",
        lua.create_async_function({
            let extract_tasks = extract_tasks.clone();
            move |_, tasks: LuaMultiValue| {
                let extract_tasks = extract_tasks.clone();
                async move {
                    let tasks = extract_tasks(tasks)?;
                    let mut futures = Vec::new();
                    for f in tasks {
                        futures.push(f.call_async::<()>(()));
                    }
                    futures::future::join_all(futures).await;
                    Ok(())
                }
            }
        })?,
    )?;

    lua.globals().set(
        "sequential",
        lua.create_async_function(move |_, tasks: LuaMultiValue| {
            let extract_tasks = extract_tasks.clone();
            async move {
                let tasks = extract_tasks(tasks)?;
                for f in tasks {
                    f.call_async::<()>(()).await?;
                }
                Ok(())
            }
        })?,
    )?;

    Ok(())
}
