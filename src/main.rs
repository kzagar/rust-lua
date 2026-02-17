mod cron;
mod drive;
mod gcp_logging;
mod gmail;
mod ibkr;
mod logger;
mod re;
mod reverse_proxy;
mod sql;
mod telegram;
mod types;
mod util;
mod web_client;
mod web_server;

use crate::types::{AppState, EngineRequest};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use mlua::prelude::*;
use mlua::serde::LuaSerdeExt;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use uuid::Uuid;

fn register_modules(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    sql::register(lua)?;
    util::register(lua)?;
    re::register(lua)?;
    // Help with finding libraries
    lua.load(r#"package.path = package.path .. ";lib/?.lua""#).exec()?;
    ibkr::register(lua)?;
    web_client::register(lua)?;
    web_server::register(lua, app_state.clone())?;
    cron::register(lua, app_state.clone())?;
    telegram::register(lua, app_state.clone())?;
    gmail::register(lua, app_state.clone())?;
    drive::register(lua, app_state.clone())?;
    reverse_proxy::register(lua, app_state.clone())?;

    // Help with random strings
    let uuid_func = lua.create_function(|_, ()| Ok(Uuid::new_v4().to_string()))?;
    lua.globals().set("uuid", uuid_func)?;

    // Register wait function
    let wait_func = lua.create_async_function(|_, seconds: f64| async move {
        tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
        Ok(())
    })?;
    lua.globals().set("wait", wait_func)?;

    // Register now function for high-res timing
    let now_func = lua.create_function(|_, ()| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        Ok(since_the_epoch.as_secs_f64())
    })?;
    lua.globals().set("now", now_func)?;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> LuaResult<()> {
    util::load_secrets();
    logger::SimpleLogger::init();

    let args: Vec<String> = std::env::args().collect();
    let path_str = if args.len() > 1 {
        &args[1]
    } else {
        "examples/example.lua"
    };
    let path = Path::new(path_str);
    let abs_path = fs::canonicalize(path).map_err(|e| {
        LuaError::RuntimeError(format!("Failed to canonicalize path {}: {}", path_str, e))
    })?;
    println!("Watching file: {:?}", abs_path);

    let (tx, mut rx) = mpsc::channel(1);

    // Setup SIGHUP signal handler for reload
    let mut sighup = signal(SignalKind::hangup())
        .map_err(|e| LuaError::RuntimeError(format!("Failed to setup SIGHUP handler: {}", e)))?;

    let tx_sighup = tx.clone();
    tokio::spawn(async move {
        loop {
            sighup.recv().await;
            println!("SIGHUP received, reloading...");
            let _ = tx_sighup.send(()).await;
        }
    });

    let lua = Lua::new();
    let gmail_state = match gmail::init_gmail_state().await {
        Ok(state) => Some(state),
        Err(e) => {
            eprintln!("Warning: Gmail not initialized: {}", e);
            eprintln!("To enable Gmail support, create a '.secrets' file in the root directory with:");
            eprintln!("  GOOGLE_CLIENT_SECRET=/path/to/your/google_client_secrets.json");
            eprintln!("  GMAIL_ATTACHMENT_DIR=attachments (Optional)");
            None
        }
    };

    // Cleanup attachments at startup
    if let Some(gs) = &gmail_state {
        let dir = &gs.attachment_manager.dir;
        if dir.exists() {
            println!("Cleaning up attachment directory: {:?}", dir);
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }

    let app_state = Arc::new(Mutex::new(AppState {
        routes: Vec::new(),
        static_routes: Vec::new(),
        cron_jobs: Vec::new(),
        reverse_proxies: Vec::new(),
        telegram_handler: None,
        config: None,
        gmail_state: gmail_state.clone(),
        drive_state: gmail_state,
        engine_tx: None,
    }));
    register_modules(&lua, app_state.clone())?;

    loop {
        // Drain pending reload signals
        while rx.try_recv().is_ok() {}

        {
            let mut state = app_state.lock().unwrap();
            state.routes.clear();
            state.static_routes.clear();
            state.cron_jobs.clear();
            state.reverse_proxies.clear();
            state.telegram_handler = None;
            state.config = None;
            state.engine_tx = None;
        }

        let content = fs::read_to_string(&abs_path)
            .map_err(|e| LuaError::RuntimeError(format!("Failed to read {}: {}", path_str, e)))?;

        println!("--- Running Lua script: {} ---", path_str);

        let run_fut = lua.load(&content).call_async::<()>(());

        tokio::select! {
            res = run_fut => {
                if let Err(e) = res {
                    eprintln!("Lua execution error: {}", e);
                    // On error, wait for change
                    let _ = rx.recv().await;
                } else {
                    println!("--- Lua script finished ---");

                    // Check if we should start server/cron/telegram logic
                    let should_run = {
                        let state = app_state.lock().unwrap();
                        !state.routes.is_empty()
                            || !state.static_routes.is_empty()
                            || !state.cron_jobs.is_empty()
                            || !state.reverse_proxies.is_empty()
                            || state.telegram_handler.is_some()
                            || state.gmail_state.is_some()
                    };

                    if should_run {
                        // This creates the engine request channel
                        let (tx_engine, mut req_rx) = mpsc::channel::<EngineRequest>(100);

                        // Start Web Server
                        let server_guard_opt =
                            web_server::start(app_state.clone(), tx_engine.clone(), abs_path.clone())
                                .await;

                        // Start Cron Scheduler
                        let sched_opt = cron::start(app_state.clone(), tx_engine.clone()).await;

                        // Start Telegram Bot
                        let tg_opt = telegram::start(app_state.clone(), tx_engine.clone()).await;

                        if server_guard_opt.is_some() || sched_opt.is_some() || tg_opt.is_some() {
                            if server_guard_opt.is_some() {
                                println!("Web Server running. Waiting for changes...");
                            }
                            if sched_opt.is_some() {
                                println!("Cron Scheduler running. Waiting for changes...");
                            }
                            if tg_opt.is_some() {
                                println!("Telegram Bot running. Waiting for changes...");
                            }

                            let mut pending_requests: FuturesUnordered<Pin<Box<dyn std::future::Future<Output = ()>>>> = FuturesUnordered::new();

                            loop {
                                tokio::select! {
                                    Some(req_enum) = req_rx.recv() => {
                                        match req_enum {
                                            EngineRequest::Rest(req) => {
                                                // Handle REST request
                                                let func_res: LuaResult<LuaFunction> = {
                                                    let state = app_state.lock().unwrap();
                                                    if req.callback_id >= state.routes.len() {
                                                         Err(LuaError::RuntimeError("Invalid callback ID".into()))
                                                    } else {
                                                        let route = &state.routes[req.callback_id];
                                                        let func: LuaFunction = lua.registry_value(&route.callback_key)?;
                                                        Ok(func)
                                                    }
                                                };

                                                match func_res {
                                                    Ok(func) => {
                                                        let params = req.params;
                                                        let response_tx = req.response_tx;
                                                        let lua_ref = &lua;

                                                        // Create future for the request
                                                        let fut = async move {
                                                            let res: LuaResult<serde_json::Value> = (async {
                                                                let params_table = lua_ref.create_table()?;
                                                                for (k, v) in params {
                                                                    params_table.set(k, v)?;
                                                                }
                                                                let val: LuaValue = func.call_async(params_table).await?;
                                                                let json_val: serde_json::Value = lua_ref.from_value(val)?;
                                                                Ok(json_val)
                                                            }).await;

                                                            match res {
                                                                Ok(val) => { response_tx.send(Ok(val)).ok(); },
                                                                Err(e) => { response_tx.send(Err(e.to_string())).ok(); }
                                                            }
                                                        };
                                                        pending_requests.push(Box::pin(fut));
                                                    },
                                                    Err(e) => {
                                                        req.response_tx.send(Err(e.to_string())).ok();
                                                    }
                                                }
                                            },
                                            EngineRequest::Cron(id) => {
                                                // Handle Cron request
                                                let func_res: LuaResult<LuaFunction> = {
                                                    let state = app_state.lock().unwrap();
                                                    if id >= state.cron_jobs.len() {
                                                        Err(LuaError::RuntimeError(
                                                            "Invalid cron callback ID".into(),
                                                        ))
                                                    } else {
                                                        let job = &state.cron_jobs[id];
                                                        let func: LuaFunction =
                                                            lua.registry_value(&job.callback_key)?;
                                                        Ok(func)
                                                    }
                                                };

                                                if let Ok(func) = func_res {
                                                    let _lua_ref = &lua;
                                                    let fut = async move {
                                                        // Call Lua function with no arguments
                                                        if let Err(e) =
                                                            func.call_async::<()>(()).await
                                                        {
                                                            eprintln!(
                                                                "Error executing cron job: {}",
                                                                e
                                                            );
                                                        }
                                                    };
                                                    pending_requests.push(Box::pin(fut));
                                                } else {
                                                    eprintln!(
                                                        "Failed to retrieve cron callback function"
                                                    );
                                                }
                                            }
                                            EngineRequest::TelegramUpdate(update) => {
                                                // Handle Telegram update
                                                let func_res: LuaResult<LuaFunction> = {
                                                    let state = app_state.lock().unwrap();
                                                    if let Some(ref key) = state.telegram_handler {
                                                        lua.registry_value(key)
                                                    } else {
                                                        Err(LuaError::RuntimeError(
                                                            "No telegram handler registered"
                                                                .into(),
                                                        ))
                                                    }
                                                };

                                                if let Ok(func) = func_res {
                                                    let lua_ref = &lua;
                                                    let fut = async move {
                                                        let update_val = lua_ref
                                                            .to_value(&update)
                                                            .unwrap_or(LuaValue::Nil);
                                                        if let Err(e) =
                                                            func.call_async::<()>(update_val).await
                                                        {
                                                            eprintln!(
                                                                "Error executing telegram handler: {}",
                                                                e
                                                            );
                                                        }
                                                    };
                                                    pending_requests.push(Box::pin(fut));
                                                } else {
                                                    eprintln!(
                                                        "Failed to retrieve telegram callback function"
                                                    );
                                                }
                                            }
                                            EngineRequest::ProxyAuth(req) => {
                                                let func: LuaFunction = match lua
                                                    .registry_value(&req.callback_key)
                                                {
                                                    Ok(f) => f,
                                                    Err(e) => {
                                                        req.response_tx.send(false).ok();
                                                        eprintln!(
                                                            "Failed to get proxy auth callback: {}",
                                                            e
                                                        );
                                                        continue;
                                                    }
                                                };
                                                let email = req.email;
                                                let domain = req.domain;
                                                let response_tx = req.response_tx;

                                                let fut = async move {
                                                    let res: LuaResult<LuaValue> =
                                                        func.call_async((email, domain)).await;
                                                    match res {
                                                        Ok(val) => {
                                                            let allowed = match val {
                                                                LuaValue::Boolean(b) => b,
                                                                _ => false,
                                                            };
                                                            response_tx.send(allowed).ok();
                                                        }
                                                        Err(e) => {
                                                            eprintln!(
                                                                "Error in proxy auth callback: {}",
                                                                e
                                                            );
                                                            response_tx.send(false).ok();
                                                        }
                                                    }
                                                };
                                                pending_requests.push(Box::pin(fut));
                                            }
                                        }
                                    }
                                    Some(_) = pending_requests.next() => {
                                        // A request finished
                                    }
                                    _ = rx.recv() => {
                                        println!("Reload signal received.");
                                        break; // Break logic loop to reload
                                    }
                                }
                            }
                        } else {
                             // Failed to start server/cron
                             println!("Waiting for changes to {}...", path_str);
                             let _ = rx.recv().await;
                        }
                    } else {
                         // Script finished cleanly with no background tasks
                         println!("No endpoints registered. Script finished.");
                         break;
                    }
                }
            }
            _ = rx.recv() => {
               // Reloading
               println!("Reload signal received (during execution).");
            }
        }
    }

    Ok(())
}
