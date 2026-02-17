use crate::types::{
    AppState, EngineRequest, ProxyAuthRequest, RestRequest, RestRouteInfo, ReverseProxyInfo,
    ServerConfig,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query as AxQuery, Request, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
};
use axum_extra::extract::Host;
use axum_extra::extract::cookie::{Cookie, CookieJar};
use axum_server::tls_openssl::OpenSSLConfig as OpenSslConfig;
use mlua::prelude::*;
use rusqlite::params;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::io::Read;
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
        !state.routes.is_empty()
            || !state.static_routes.is_empty()
            || !state.reverse_proxies.is_empty()
            || state.gmail_state.is_some()
    };

    if should_run {
        println!("Starting server...");
        let config = {
            let mut state = app_state.lock().unwrap();
            state.engine_tx = Some(tx.clone());
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

        let mut router = Router::new().route("/auth/google/callback", get(handle_google_callback));

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

        let router = router.fallback(proxy_handler).with_state(app_state.clone());

        match config {
            Some(ServerConfig::Http(addr)) => {
                println!("REST server listening on http://{}", addr);
                let listener_res = tokio::net::TcpListener::bind(&addr).await;
                match listener_res {
                    Ok(listener) => {
                        let server_handle = tokio::spawn(async move {
                            if let Err(e) = axum::serve(listener, router.into_make_service()).await
                            {
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
                    OpenSslConfig::from_pem_file(PathBuf::from(cert), PathBuf::from(key));
                match config_res {
                    Ok(tls_config) => {
                        let addr_parsed: Result<std::net::SocketAddr, _> = addr.parse();
                        match addr_parsed {
                            Ok(socket_addr) => {
                                let server_handle = tokio::spawn(async move {
                                    if let Err(e) =
                                        axum_server::bind_openssl(socket_addr, tls_config)
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

async fn handle_google_callback(
    State(app_state): State<Arc<Mutex<AppState>>>,
    jar: CookieJar,
    AxQuery(params): AxQuery<HashMap<String, String>>,
) -> impl IntoResponse {
    let code = match params.get("code") {
        Some(c) => c.clone(),
        None => return "Missing code".into_response(),
    };
    let state_param = match params.get("state") {
        Some(s) => s.clone(),
        None => return "Missing state".into_response(),
    };

    let gmail_state = {
        let state = app_state.lock().unwrap();
        state.gmail_state.clone()
    };

    let gs = match gmail_state {
        Some(s) => s,
        None => return "Gmail not initialized".into_response(),
    };

    // If state_param looks like an email, it's the old GMail linking flow
    if state_param.contains('@') && !state_param.contains('/') {
        match crate::gmail::handle_callback(gs, code).await {
            Ok(email) => format!(
                "Authentication successful as {}! You can close this window.",
                email
            )
            .into_response(),
            Err(e) => format!("Authentication failed: {}", e).into_response(),
        }
    } else {
        // General login flow for proxy
        let config = &gs.config;
        let res = match tokio::task::spawn_blocking({
            let client_id = config.client_id.clone();
            let client_secret = config.client_secret.clone();
            let redirect_uri = config.redirect_uri.clone();
            move || {
                ureq::post("https://oauth2.googleapis.com/token")
                    .set("Content-Type", "application/x-www-form-urlencoded")
                    .send_string(&format!(
                        "client_id={}&client_secret={}&code={}&grant_type=authorization_code&redirect_uri={}",
                        client_id,
                        client_secret,
                        code,
                        urlencoding::encode(&redirect_uri)
                    ))
            }
        }).await {
            Ok(Ok(r)) => r,
            _ => return "Failed to exchange token".into_response(),
        };

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            _scope: Option<String>,
        }

        let token_res: TokenResponse = match res.into_json() {
            Ok(t) => t,
            Err(e) => return format!("Failed to parse token response: {}", e).into_response(),
        };

        // Get email
        let access_token = token_res.access_token.clone();
        let email_res = tokio::task::spawn_blocking(move || {
            ureq::get("https://www.googleapis.com/oauth2/v3/userinfo")
                .set("Authorization", &format!("Bearer {}", access_token))
                .call()
        })
        .await;

        let email_json: serde_json::Value = match email_res {
            Ok(Ok(r)) => r.into_json().unwrap_or_default(),
            _ => return "Failed to get user info".into_response(),
        };

        let email = match email_json.get("email").and_then(|v| v.as_str()) {
            Some(e) => e.to_string(),
            None => return "Failed to get email from Google".into_response(),
        };

        // Set cookies
        let jar = jar
            .add(Cookie::build(("rua_email", email)).path("/").build())
            .add(
                Cookie::build(("rua_access_token", token_res.access_token))
                    .path("/")
                    .build(),
            );

        let redirect_to = if state_param.starts_with('/') {
            state_param
        } else {
            "/".to_string()
        };

        (jar, Redirect::to(&redirect_to)).into_response()
    }
}

async fn proxy_handler(
    State(app_state): State<Arc<Mutex<AppState>>>,
    jar: CookieJar,
    Host(host): Host,
    req: Request,
) -> Response {
    let matched_proxy = {
        let state = app_state.lock().unwrap();
        state
            .reverse_proxies
            .iter()
            .find(|p| p.host == host && req.uri().path().starts_with(&p.path_prefix))
            .cloned()
    };

    if let Some(proxy) = matched_proxy {
        if let Some(domain) = &proxy.domain {
            let email = match jar.get("rua_email") {
                Some(c) => c.value().to_string(),
                None => {
                    // Redirect to login
                    let gmail_state = {
                        let state = app_state.lock().unwrap();
                        state.gmail_state.clone()
                    };
                    let gs = match gmail_state {
                        Some(s) => s,
                        None => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Gmail not initialized for auth",
                            )
                                .into_response();
                        }
                    };

                    let mut auth_url =
                        url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
                    {
                        let mut query = auth_url.query_pairs_mut();
                        query.append_pair("client_id", &gs.config.client_id);
                        query.append_pair("redirect_uri", &gs.config.redirect_uri);
                        query.append_pair("response_type", "code");
                        query.append_pair("scope", "openid email");
                        query.append_pair("access_type", "online");
                        query.append_pair("state", req.uri().path()); // Store current path to redirect back
                    }
                    return Redirect::to(auth_url.as_str()).into_response();
                }
            };

            // Check authorization
            let is_authorized = check_authorization(&app_state, &proxy, &email, domain).await;
            if !is_authorized {
                return (StatusCode::FORBIDDEN, "Unauthorized").into_response();
            }
        }

        // Forward request
        forward_request(proxy, req).await.into_response()
    } else {
        (StatusCode::NOT_FOUND, "Not Found").into_response()
    }
}

async fn check_authorization(
    app_state: &Arc<Mutex<AppState>>,
    proxy: &ReverseProxyInfo,
    email: &str,
    domain: &str,
) -> bool {
    // 1. Try Lua callback
    if let Some(callback_key) = &proxy.auth_callback {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let engine_tx = {
            let state = app_state.lock().unwrap();
            state.engine_tx.clone()
        };

        if let Some(engine_tx) = engine_tx {
            let req = ProxyAuthRequest {
                callback_key: Arc::clone(callback_key),
                email: email.to_string(),
                domain: domain.to_string(),
                response_tx: tx,
            };

            #[allow(clippy::collapsible_if)]
            if engine_tx.send(EngineRequest::ProxyAuth(req)).await.is_ok() {
                if let Ok(allowed) = rx.await {
                    return allowed;
                }
            }
        }
    }

    // 2. Try SQLite
    let domain = domain.to_string();
    let email = email.to_string();
    tokio::task::spawn_blocking(move || match rusqlite::Connection::open("server.db") {
        Ok(conn) => {
            let res: Result<i32, _> = conn.query_row(
                "SELECT 1 FROM authorized_users WHERE domain = ? AND email = ?",
                params![domain, email],
                |_| Ok(1),
            );
            res.is_ok()
        }
        Err(_) => false,
    })
    .await
    .unwrap_or(false)
}

async fn forward_request(proxy: ReverseProxyInfo, req: Request) -> Response {
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let url = format!("{}{}{}", proxy.remote_base, path, query);

    let method = req.method().to_string();

    let mut headers = HashMap::new();
    for (name, value) in req.headers() {
        if name == "host" {
            continue;
        }
        if let Ok(v) = value.to_str() {
            headers.insert(name.as_str().to_string(), v.to_string());
        }
    }
    if let Some(h) = req.headers().get("host").and_then(|v| v.to_str().ok()) {
        headers.insert("X-Forwarded-Host".to_string(), h.to_string());
    }

    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to read body").into_response(),
    };

    let res = match tokio::task::spawn_blocking(move || {
        let mut ureq_req = match method.as_str() {
            "GET" => ureq::get(&url),
            "POST" => ureq::post(&url),
            "PUT" => ureq::put(&url),
            "DELETE" => ureq::delete(&url),
            "PATCH" => ureq::patch(&url),
            _ => return Err("Unsupported method".to_string()),
        };

        for (k, v) in headers {
            ureq_req = ureq_req.set(&k, &v);
        }
        ureq_req.send_bytes(&body_bytes).map_err(|e| e.to_string())
    })
    .await
    {
        Ok(Ok(r)) => r,
        _ => return (StatusCode::BAD_GATEWAY, "Proxy error").into_response(),
    };

    let status = res.status();
    let mut response_builder = Response::builder().status(status as u16);

    // Copy headers from ureq response to axum response
    for name in res.headers_names() {
        if let Some(value) = res.header(&name) {
            response_builder = response_builder.header(name, value);
        }
    }

    let mut response_body = Vec::new();
    if res.into_reader().read_to_end(&mut response_body).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read response body",
        )
            .into_response();
    }

    response_builder
        .body(Body::from(response_body))
        .unwrap_or_else(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build response",
            )
                .into_response()
        })
}
