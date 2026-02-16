use crate::types::{AppState, EngineRequest, RestRequest, RestRouteInfo, ServerConfig};
use axum::{
    Json, Router,
    extract::Query as AxQuery,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use axum_server::tls_rustls::RustlsConfig;
use mlua::prelude::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc::Sender, oneshot};
use tower_http::services::ServeDir;

pub struct RestServer {
    state: Arc<Mutex<AppState>>,
}

impl LuaUserData for RestServer {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "register",
            |lua, server, (path, method, func): (String, String, LuaFunction)| {
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
            },
        );

        methods.add_method("listen", |_, server, addr: String| {
            let mut state = server.state.lock().unwrap();
            state.config = Some(ServerConfig::Http(addr));
            Ok(())
        });

        methods.add_method(
            "listen_tls",
            |_, server, (addr, cert_path, key_path): (String, String, String)| {
                let mut state = server.state.lock().unwrap();
                state.config = Some(ServerConfig::Https(addr, cert_path, key_path));
                Ok(())
            },
        );

        methods.add_method(
            "serve_static",
            |_, server, (url_path, fs_path): (String, String)| {
                let mut state = server.state.lock().unwrap();
                state.static_routes.push((url_path, fs_path));
                Ok(())
            },
        );
    }
}

pub struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn register(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    // Register rest module
    let rest = lua.create_table()?;
    let state_clone = app_state.clone();
    rest.set(
        "new",
        lua.create_function(move |_, ()| {
            Ok(RestServer {
                state: state_clone.clone(),
            })
        })?,
    )?;
    lua.globals().set("rest", rest)?;

    Ok(())
}

pub async fn start(
    app_state: Arc<Mutex<AppState>>,
    tx: Sender<EngineRequest>,
    abs_path: PathBuf,
) -> Option<ServerGuard> {
    let script_dir = abs_path.parent().unwrap_or(Path::new("."));

    // Check if we should start server
    let should_run = {
        let state = app_state.lock().unwrap();
        !state.routes.is_empty() || !state.static_routes.is_empty()
    };

    if should_run {
        println!("Starting server...");
        let config = {
            let mut state = app_state.lock().unwrap();
            if state.config.is_none() {
                println!("Using default configuration: HTTPS 0.0.0.0:3443");
                state.config = Some(ServerConfig::Https(
                    "0.0.0.0:3443".to_string(),
                    "cert.pem".to_string(),
                    "key.pem".to_string(),
                ));
            }
            state.config.clone()
        };

        let mut router = Router::new();

        // Setup routes
        {
            let state = app_state.lock().unwrap();
            // Setup static routes
            for (url_path, fs_path_str) in &state.static_routes {
                let full_fs_path = script_dir.join(fs_path_str);
                match std::fs::canonicalize(&full_fs_path) {
                    Ok(real_path) => {
                        if real_path.is_symlink() {
                            eprintln!(
                                "Warning: Skipping static path {} -> {} (Symlink detected)",
                                url_path, fs_path_str
                            );
                            continue;
                        }
                        println!("Serving static: {} -> {:?}", url_path, real_path);

                        let service = ServeDir::new(real_path);
                        if url_path == "/" {
                            router = router.fallback_service(service);
                        } else {
                            router = router.nest_service(url_path, service);
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Static path {} not found: {} ({})",
                            fs_path_str,
                            full_fs_path.display(),
                            e
                        );
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

                    if tx_clone.send(EngineRequest::Rest(req)).await.is_err() {
                        return (StatusCode::INTERNAL_SERVER_ERROR, "Server shutting down")
                            .into_response();
                    }

                    match res_rx.await {
                        Ok(Ok(val)) => Json::<JsonValue>(val).into_response(),
                        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
                        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "No response from Lua")
                            .into_response(),
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

        match config {
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
                    }
                    Err(e) => {
                        eprintln!("Failed to bind to {}: {}", addr, e);
                        None
                    }
                }
            }
            Some(ServerConfig::Https(addr, cert, key)) => {
                println!("REST server listening (TLS) on https://{}", addr);
                let config_res =
                    RustlsConfig::from_pem_file(PathBuf::from(cert), PathBuf::from(key)).await;
                match config_res {
                    Ok(tls_config) => {
                        let addr_parsed: Result<std::net::SocketAddr, _> = addr.parse();
                        match addr_parsed {
                            Ok(socket_addr) => {
                                let server_handle = tokio::spawn(async move {
                                    if let Err(e) =
                                        axum_server::bind_rustls(socket_addr, tls_config)
                                            .serve(router.into_make_service())
                                            .await
                                    {
                                        eprintln!("REST server error: {}", e);
                                    }
                                });
                                Some(ServerGuard(server_handle))
                            }
                            Err(e) => {
                                eprintln!("Invalid address {}: {}", addr, e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("TLS config error: {}", e);
                        None
                    }
                }
            }
            None => {
                // This shouldn't happen because we set default config above if none
                None
            }
        }
    } else {
        None
    }
}
