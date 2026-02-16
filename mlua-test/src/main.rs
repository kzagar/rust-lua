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

struct RestServer {
    routes: Vec<RestRouteInfo>,
}

struct RestRouteInfo {
    path: String,
    method: String,
    callback_id: usize,
    callback_key: RegistryKey,
}

struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl LuaUserData for RestServer {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("register", |lua, server, (path, method, func): (String, String, LuaFunction)| {
            let callback_id = server.routes.len();
            let callback_key = lua.create_registry_value(func)?;
            server.routes.push(RestRouteInfo {
                path,
                method: method.to_uppercase(),
                callback_id,
                callback_key,
            });
            Ok(())
        });

        methods.add_async_method("listen", |lua, server, addr: String| async move {
            let (tx, mut rx) = mpsc::channel::<RestRequest>(100);

            let mut router = Router::new();
            
            // We need to move the routes info into the Axum handlers.
            // Since Axum handlers need to be Send, we'll store the callback_ids.
            for route_info in &server.routes {
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
                    _ => router.route(&path, get(handler)), // Default to GET
                };
            }

            println!("REST server listening on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            
            // Run axum in a separate task
            let server_handle = tokio::spawn(async move {
                if let Err(e) = axum::serve(listener, router).await {
                    eprintln!("REST server error: {}", e);
                }
            });
            let _guard = ServerGuard(server_handle);

            // Process requests in the Lua thread
            loop {
                tokio::select! {
                    Some(req) = rx.recv() => {
                        let route_info = &server.routes[req.callback_id];
                        let func: LuaFunction = lua.registry_value(&route_info.callback_key)?;
                        
                        // Convert params to Lua table
                        let params_table = lua.create_table()?;
                        for (k, v) in req.params {
                            params_table.set(k, v)?;
                        }

                        // Call Lua function
                        let res: LuaResult<LuaValue> = func.call_async(params_table).await;
                        match res {
                            Ok(val) => {
                                // Convert Lua value back to JSON
                                let json_val: std::result::Result<JsonValue, _> = lua.from_value(val);
                                match json_val {
                                    Ok(jv) => { req.response_tx.send(Ok(jv)).ok(); },
                                    Err(e) => { req.response_tx.send(Err(format!("JSON conversion error: {}", e))).ok(); }
                                }
                            },
                            Err(e) => {
                                req.response_tx.send(Err(e.to_string())).ok();
                            }
                        }
                    }
                    else => break,
                }
            }

            // _guard will be dropped here, aborting the server
            Ok(())
        });

        methods.add_async_method("listen_tls", |lua, server, (addr, cert_path, key_path): (String, String, String)| async move {
            let config = RustlsConfig::from_pem_file(
                PathBuf::from(cert_path),
                PathBuf::from(key_path),
            )
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

            let (tx, mut rx) = mpsc::channel::<RestRequest>(100);
            let mut router = Router::new();
            
            for route_info in &server.routes {
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

            println!("REST server listening (TLS) on https://{}", addr);
            let addr_parsed: std::net::SocketAddr = addr.parse()
                .map_err(|e: std::net::AddrParseError| LuaError::RuntimeError(e.to_string()))?;
            
            let server_handle = tokio::spawn(async move {
                if let Err(e) = axum_server::bind_rustls(addr_parsed, config)
                    .serve(router.into_make_service())
                    .await {
                    eprintln!("REST server error: {}", e);
                }
            });
            let _guard = ServerGuard(server_handle);

            loop {
                tokio::select! {
                    Some(req) = rx.recv() => {
                        let route_info = &server.routes[req.callback_id];
                        let func: LuaFunction = lua.registry_value(&route_info.callback_key)?;
                        
                        let params_table = lua.create_table()?;
                        for (k, v) in req.params {
                            params_table.set(k, v)?;
                        }

                        let res: LuaResult<LuaValue> = func.call_async(params_table).await;
                        match res {
                            Ok(val) => {
                                let json_val: std::result::Result<JsonValue, _> = lua.from_value(val);
                                match json_val {
                                    Ok(jv) => { req.response_tx.send(Ok(jv)).ok(); },
                                    Err(e) => { req.response_tx.send(Err(format!("JSON conversion error: {}", e))).ok(); }
                                }
                            },
                            Err(e) => {
                                req.response_tx.send(Err(e.to_string())).ok();
                            }
                        }
                    }
                    else => break,
                }
            }

            // _guard will be dropped here, aborting the server
            Ok(())
        });
    }
}

fn register_modules(lua: &Lua) -> LuaResult<()> {
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
    rest.set("new", lua.create_function(|_, ()| {
        Ok(RestServer { routes: Vec::new() })
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
    register_modules(&lua)?;

    loop {
        // Drain pending reload signals
        while let Ok(_) = rx.try_recv() {}

        let content = fs::read_to_string(&abs_path)
            .map_err(|e| LuaError::RuntimeError(format!("Failed to read {}: {}", path_str, e)))?;

        println!("--- Running Lua script: {} ---", path_str);
        
        let run_fut = lua.load(&content).call_async::<()>(());
        
        tokio::select! {
            res = run_fut => {
                if let Err(e) = res {
                    eprintln!("Lua execution error: {}", e);
                } else {
                    println!("--- Lua script finished ---");
                    break;
                }
                println!("Waiting for changes to {}...", path_str);
                // Wait for a change before restarting
                let _ = rx.recv().await;
            }
            _ = rx.recv() => {
               // println!("--- Reloading Lua script: {} ---", path_str);
            }
        }
    }

    Ok(())
}
