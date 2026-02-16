use mlua::prelude::*;
use sqlx::{sqlite::SqlitePoolOptions, Row, Column};
use std::fs;
use uuid::Uuid;
use reqwest::{Client, Method, header::HeaderMap};
use serde_json::Value as JsonValue;
use axum::{
    extract::Query as AxQuery,
    response::IntoResponse,
    routing::{get, post, delete, put},
    Json, Router,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use axum_server::tls_rustls::RustlsConfig;
use std::path::PathBuf;
use mlua::RegistryKey;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use std::time::Duration;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tower_http::services::ServeDir;
use futures::stream::FuturesUnordered;
use futures::StreamExt;

struct Database {
    pool: sqlx::SqlitePool,
}

impl LuaUserData for Database {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("exec", |_, db, sql: String| async move {
            sqlx::query(&sql)
                .execute(&db.pool)
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(())
        });

        methods.add_async_method("close", |_, db, ()| async move {
            db.pool.close().await;
            Ok(())
        });

        methods.add_async_method("rows", |lua, db, sql: String| async move {
            let sql_results = sqlx::query(&sql)
                .fetch_all(&db.pool)
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let mut rows = Vec::new();
            for sql_row in sql_results {
                let table = lua.create_table()?;
                for column in sql_row.columns() {
                    let name = column.name();
                    // Basic type handling: try string first, then handle others if needed.
                    // For this test, we'll assume we can get it as a string or handle nulls.
                    let val: Option<String> = sql_row.try_get(name).ok();
                    match val {
                        Some(v) => table.set(name, v)?,
                        None => {
                            // Try to get as i64 if it's an integer
                            let val_int: Option<i64> = sql_row.try_get(name).ok();
                            match val_int {
                                Some(v) => table.set(name, v)?,
                                None => table.set(name, LuaValue::Nil)?,
                            }
                        }
                    }
                }
                rows.push(table);
            }

            let index = std::cell::Cell::new(0);
            let iterator = lua.create_function(move |_, ()| {
                let curr = index.get();
                if curr < rows.len() {
                    let row = rows[curr].clone();
                    index.set(curr + 1);
                    Ok(Some(row))
                } else {
                    Ok(None)
                }
            })?;

            Ok(iterator)
        });
    }
}

struct HttpClient {
    client: Client,
}

impl LuaUserData for HttpClient {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("request_uri", |lua, client, (url, options): (String, Option<LuaTable>)| async move {
            let mut req = client.client.get(&url); // Default to GET

            if let Some(opts) = options {
                if let Some(method_str) = opts.get::<Option<String>>("method")? {
                    let method = Method::from_bytes(method_str.to_uppercase().as_bytes())
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                    req = client.client.request(method, &url);
                }

                if let Some(body) = opts.get::<Option<String>>("body")? {
                    req = req.body(body);
                }

                if let Some(headers) = opts.get::<Option<LuaTable>>("headers")? {
                    let mut headermap = HeaderMap::new();
                    for pair in headers.pairs::<String, String>() {
                        let (k, v) = pair?;
                        headermap.insert(
                            reqwest::header::HeaderName::from_bytes(k.as_bytes())
                                .map_err(|e| LuaError::RuntimeError(e.to_string()))?,
                            reqwest::header::HeaderValue::from_str(&v)
                                .map_err(|e| LuaError::RuntimeError(e.to_string()))?,
                        );
                    }
                    req = req.headers(headermap);
                }
            }

            let resp = req.send().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let status = resp.status().as_u16();
            let headers = lua.create_table()?;
            for (name, value) in resp.headers() {
                headers.set(name.as_str(), value.to_str().unwrap_or(""))?;
            }
            let body = resp.text().await.map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let res_table = lua.create_table()?;
            res_table.set("status", status)?;
            res_table.set("headers", headers)?;
            res_table.set("body", body)?;

            Ok((res_table, LuaValue::Nil))
        });
    }
}

struct RestRequest {
    callback_id: usize,
    params: HashMap<String, String>,
    response_tx: oneshot::Sender<Result<JsonValue, String>>,
}



struct RestRouteInfo {
    path: String,
    method: String,
    callback_id: usize,
    callback_key: RegistryKey,
}

#[derive(Clone)]
enum ServerConfig {
    Http(String),
    Https(String, String, String),
}

struct AppState {
    routes: Vec<RestRouteInfo>,
    static_routes: Vec<(String, String)>,
    config: Option<ServerConfig>,
}

struct RestServer {
    state: Arc<Mutex<AppState>>,
}

struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl LuaUserData for RestServer {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("register", |lua, server, (path, method, func): (String, String, LuaFunction)| {
            let mut state = server.state.lock().unwrap();
            let callback_id = state.routes.len();
            let callback_key = lua.create_registry_value(func)?;
            state.routes.push(RestRouteInfo {
                path,
                method: method.to_uppercase(),
                callback_id,
                callback_key,
            });
            Ok(())
        });

        methods.add_method("listen", |_, server, addr: String| {
            let mut state = server.state.lock().unwrap();
            state.config = Some(ServerConfig::Http(addr));
            Ok(())
        });


        methods.add_method("listen_tls", |_, server, (addr, cert_path, key_path): (String, String, String)| {
            let mut state = server.state.lock().unwrap();
            state.config = Some(ServerConfig::Https(addr, cert_path, key_path));
            Ok(())
        });

        methods.add_method("serve_static", |_, server, (url_path, fs_path): (String, String)| {
             let mut state = server.state.lock().unwrap();
             state.static_routes.push((url_path, fs_path));
             Ok(())
        });
    }
}

fn register_modules(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    // Register sqlite3 module
    let sqlite3 = lua.create_table()?;
    sqlite3.set("open", lua.create_async_function(|_, path: String| async move {
        use std::str::FromStr;
        let options = sqlx::sqlite::SqliteConnectOptions::from_str(&format!("sqlite://{}", path))
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        Ok(Database { pool })
    })?)?;
    lua.globals().set("sqlite3", sqlite3)?;

    // Help with random strings
    let uuid_func = lua.create_function(|_, ()| {
        Ok(Uuid::new_v4().to_string())
    })?;
    lua.globals().set("uuid", uuid_func)?;

    // Register wait function
    let wait_func = lua.create_async_function(|_, seconds: f64| async move {
        tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
        Ok(())
    })?;
    lua.globals().set("wait", wait_func)?;

    // Register http module (lua-resty-http compatible)
    let http = lua.create_table()?;
    http.set("new", lua.create_function(|_, ()| {
        Ok(HttpClient { client: Client::new() })
    })?)?;
    lua.globals().set("http", http)?;

    // Register json module
    let json = lua.create_table()?;
    json.set("encode", lua.create_function(|lua, value: LuaValue| {
        let json_val: JsonValue = lua.from_value(value)?;
        serde_json::to_string(&json_val).map_err(|e| LuaError::RuntimeError(e.to_string()))
    })?)?;
    json.set("decode", lua.create_function(|lua, s: String| {
        let json_val: JsonValue = serde_json::from_str(&s).map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        lua.to_value(&json_val)
    })?)?;
    lua.globals().set("json", json)?;

    // Register rest module
    let rest = lua.create_table()?;
    let state_clone = app_state.clone();
    rest.set("new", lua.create_function(move |_, ()| {
        Ok(RestServer { state: state_clone.clone() })
    })?)?;
    lua.globals().set("rest", rest)?;

    Ok(())
}

#[tokio::main]
async fn main() -> LuaResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let path_str = if args.len() > 1 {
        &args[1]
    } else {
        "example.lua"
    };
    let path = Path::new(path_str);
    let abs_path = fs::canonicalize(path).map_err(|e| LuaError::RuntimeError(format!("Failed to canonicalize path {}: {}", path_str, e)))?;
    println!("Watching file: {:?}", abs_path);

    let (tx, mut rx) = mpsc::channel(1);

    // Setup file watcher
    let tx_clone = tx.clone();
    let abs_path_clone = abs_path.clone();
    let mut last_mtime = std::time::SystemTime::UNIX_EPOCH;
    
    // Initial mtime
    if let Ok(metadata) = fs::metadata(&abs_path) {
        if let Ok(mtime) = metadata.modified() {
            last_mtime = mtime;
        }
    }

    let mut _debouncer = new_debouncer(Duration::from_millis(500), move |res: std::result::Result<Vec<_>, _>| {
        match res {
            Ok(events) => {
                let mut reload = false;
                for event in events {
                    println!("File event: {:?}", event);
                    // Check if file was actually modified
                     if let Ok(metadata) = fs::metadata(&abs_path_clone) {
                        if let Ok(mtime) = metadata.modified() {
                            if mtime > last_mtime {
                                last_mtime = mtime;
                                reload = true;
                            } else {
                                // Skipping reload
                            }
                        }
                    }
                }
                if reload {
                    let _ = tx_clone.blocking_send(());
                }
            },
            Err(e) => eprintln!("Watch error: {:?}", e),
        }
    }).map_err(|e| LuaError::RuntimeError(e.to_string()))?;

    _debouncer.watcher().watch(&abs_path, RecursiveMode::NonRecursive)
        .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

    let lua = Lua::new();
    let app_state = Arc::new(Mutex::new(AppState {
        routes: Vec::new(),
        static_routes: Vec::new(),
        config: None,
    }));
    register_modules(&lua, app_state.clone())?;

    loop {
        // Drain pending reload signals
        while let Ok(_) = rx.try_recv() {}

        {
            let mut state = app_state.lock().unwrap();
            state.routes.clear();
            state.static_routes.clear();
            state.config = None;
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
                } else {
                    println!("--- Lua script finished ---");
                    
                    // Check if we should start server
                    let should_run = {
                        let state = app_state.lock().unwrap();
                        !state.routes.is_empty() || !state.static_routes.is_empty()
                    };

                    if should_run {
                        // Start server logic
                        println!("Starting server...");
                        let config = {
                            let mut state = app_state.lock().unwrap();
                            if state.config.is_none() {
                                println!("Using default configuration: HTTPS 0.0.0.0:3443");
                                state.config = Some(ServerConfig::Https("0.0.0.0:3443".to_string(), "cert.pem".to_string(), "key.pem".to_string()));
                            }
                            state.config.clone()
                        };

                        let (tx, mut req_rx) = mpsc::channel::<RestRequest>(100);
                        let mut router = Router::new();
                        let script_dir = abs_path.parent().unwrap_or(Path::new("."));
                        
                        // Setup routes
                        {
                            let state = app_state.lock().unwrap();
                            // Setup static routes
                            for (url_path, fs_path_str) in &state.static_routes {
                                let mut full_fs_path = script_dir.join(fs_path_str);
                                // Security check: sanitize and ensure no symlinks escaping or parent traversal issues if possible
                                // Basic check: canonicalize and ensure it starts with script dir (or allowed dir)
                                // For now, we will canonicalize and enforce that it exists.
                                // NOTE: Enforcing "not outside directory" strictly is hard without a defined root.
                                // We will trust the Lua script path resolution relative to itself, but check for symlinks via canonicalize.
                                match fs::canonicalize(&full_fs_path) {
                                     Ok(real_path) => {
                                         if real_path.is_symlink() {
                                             eprintln!("Warning: Skipping static path {} -> {} (Symlink detected)", url_path, fs_path_str);
                                             continue;
                                         }
                                         // Check traversal? (already resolved by canonicalize)
                                         println!("Serving static: {} -> {:?}", url_path, real_path);
                                         
                                         let service = ServeDir::new(real_path);
                                         // If url_path is "/", we might want it at root.
                                         // Axum nest_service expects a path prefix.
                                         if url_path == "/" {
                                              router = router.fallback_service(service);
                                         } else {
                                              router = router.nest_service(url_path, service);
                                         }
                                     },
                                     Err(e) => {
                                         eprintln!("Warning: Static path {} not found: {} ({})", fs_path_str, full_fs_path.display(), e);
                                     }
                                }
                            }

                            for route_info in &state.routes {
                                let path = route_info.path.clone();
                                let method = route_info.method.clone();
                                let callback_id = route_info.callback_id;
                                let tx_clone = tx.clone();
    
                                let handler = move |AxQuery(params): AxQuery<HashMap<String, String>>| async move {
                                    let (res_tx, res_rx) = oneshot::channel();
                                    let req = RestRequest {
                                        callback_id,
                                        params,
                                        response_tx: res_tx,
                                    };
    
                                    if tx_clone.send(req).await.is_err() {
                                        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Server shutting down").into_response();
                                    }
    
                                    match res_rx.await {
                                        Ok(Ok(val)) => Json::<JsonValue>(val).into_response(),
                                        Ok(Err(e)) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
                                        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "No response from Lua").into_response(),
                                    }
                                };
    
                                router = match method.as_str() {
                                    "GET" => router.route(&path, get(handler)),
                                    "POST" => router.route(&path, post(handler)),
                                    "PUT" => router.route(&path, put(handler)),
                                    "DELETE" => router.route(&path, delete(handler)),
                                    _ => router.route(&path, get(handler)),
                                };
                            }
                        }

                        let server_guard_opt = match config {
                            Some(ServerConfig::Http(addr)) => {
                                println!("REST server listening on http://{}", addr);
                                let listener_res = tokio::net::TcpListener::bind(&addr).await;
                                match listener_res {
                                    Ok(listener) => {
                                        let server_handle = tokio::spawn(async move {
                                            if let Err(e) = axum::serve(listener, router).await {
                                                eprintln!("REST server error: {}", e);
                                            }
                                        });
                                        Some(ServerGuard(server_handle))
                                    },
                                    Err(e) => {
                                        eprintln!("Failed to bind to {}: {}", addr, e);
                                        None
                                    }
                                }
                            },
                             Some(ServerConfig::Https(addr, cert, key)) => {
                                println!("REST server listening (TLS) on https://{}", addr);
                                let config_res = RustlsConfig::from_pem_file(PathBuf::from(cert), PathBuf::from(key)).await;
                                match config_res {
                                    Ok(tls_config) => {
                                        let addr_parsed: Result<std::net::SocketAddr, _> = addr.parse();
                                        match addr_parsed {
                                            Ok(socket_addr) => {
                                                let server_handle = tokio::spawn(async move {
                                                    if let Err(e) = axum_server::bind_rustls(socket_addr, tls_config)
                                                        .serve(router.into_make_service())
                                                        .await {
                                                        eprintln!("REST server error: {}", e);
                                                    }
                                                });
                                                Some(ServerGuard(server_handle))
                                            },
                                            Err(e) => {
                                                 eprintln!("Invalid address {}: {}", addr, e);
                                                 None
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!("TLS config error: {}", e);
                                        None
                                    }
                                }
                            },
                            None => {
                                eprintln!("Endpoints registered but no server configuration found (call srv:listen or srv:listen_tls)");
                                None
                            }
                        };

                        if let Some(_guard) = server_guard_opt {
                             println!("Server running. Waiting for changes...");
                                let mut pending_requests = FuturesUnordered::new();
                                loop {
                                    tokio::select! {
                                        Some(req) = req_rx.recv() => {
                                            // Handle request
                                            // We do the setup synchronously to fail fast if registry lookup fails
                                            // and to capture necessary data for the future
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
                                                        let res: LuaResult<JsonValue> = (async {
                                                            let params_table = lua_ref.create_table()?;
                                                            for (k, v) in params {
                                                                params_table.set(k, v)?;
                                                            }
                                                            let val: LuaValue = func.call_async(params_table).await?;
                                                            let json_val: JsonValue = lua_ref.from_value(val)?;
                                                            Ok(json_val)
                                                        }).await;

                                                        match res {
                                                            Ok(val) => { response_tx.send(Ok(val)).ok(); },
                                                            Err(e) => { response_tx.send(Err(e.to_string())).ok(); }
                                                        }
                                                    };
                                                    pending_requests.push(fut);
                                                },
                                                Err(e) => {
                                                    // Send error immediately if setup failed
                                                    req.response_tx.send(Err(e.to_string())).ok();
                                                }
                                            }
                                        }
                                        Some(_) = pending_requests.next() => {
                                            // A request finished
                                        }
                                        _ = rx.recv() => {
                                            println!("Reload signal received.");
                                            break; // Break inner loop to reload
                                        }
                                    }
                                }
                        } else {
                            // Server failed to start
                             println!("Waiting for changes to {}...", path_str);
                             let _ = rx.recv().await;
                        }

                    } else {
                         // Script finished and no routes, just exit or wait?
                         // Original behavior was to exit if script finished.
                         // But if we are in watch mode, maybe we should wait?
                         // The prompt says "HTTPS server should be started... if endpoints registered".
                         // If no endpoints, behavior is undefined by prompt, but typically scripts might just run once.
                         // However, keeping consistent with "watch" mode:
                         if cfg!(debug_assertions) { // Just a guess, or always wait?
                            // Let's break the outer loop if no server to run, unless we want to keep watching empty scripts.
                            println!("No endpoints registered. Script finished.");
                            break; 
                         }
                         break;
                    }
                }
            }
            _ = rx.recv() => {
               // Reloading
            }
        }
    }

    Ok(())
}
