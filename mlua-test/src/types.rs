use mlua::RegistryKey;
use std::sync::Arc;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tokio::sync::oneshot as tokio_oneshot;

pub struct RestRequest {
    pub callback_id: usize,
    pub params: HashMap<String, String>,
    pub response_tx: tokio_oneshot::Sender<Result<JsonValue, String>>,
}

pub struct ProxyAuthRequest {
    pub callback_key: Arc<RegistryKey>,
    pub email: String,
    pub domain: String,
    pub response_tx: tokio_oneshot::Sender<bool>,
}

pub enum EngineRequest {
    Rest(RestRequest),
    Cron(usize),
    TelegramUpdate(JsonValue),
    ProxyAuth(ProxyAuthRequest),
}

pub struct RestRouteInfo {
    pub path: String,
    pub method: String,
    pub callback_id: usize,
    pub callback_key: RegistryKey,
}

pub struct CronJobInfo {
    pub expression: String,
    pub callback_id: usize,
    pub callback_key: RegistryKey,
}

#[derive(Clone)]
pub enum ServerConfig {
    Http(String),
    Https(String, String, String),
}

#[derive(Clone)]
pub struct ReverseProxyInfo {
    pub host: String,
    pub path_prefix: String,
    pub remote_base: String,
    pub domain: Option<String>,
    pub auth_callback: Option<Arc<RegistryKey>>,
}

pub struct AppState {
    pub routes: Vec<RestRouteInfo>,
    pub static_routes: Vec<(String, String)>,
    pub cron_jobs: Vec<CronJobInfo>,
    pub reverse_proxies: Vec<ReverseProxyInfo>,
    pub telegram_handler: Option<RegistryKey>,
    pub config: Option<ServerConfig>,
    pub gmail_state: Option<std::sync::Arc<crate::gmail::GmailState>>,
    pub drive_state: Option<std::sync::Arc<crate::gmail::GmailState>>,
    pub engine_tx: Option<tokio::sync::mpsc::Sender<EngineRequest>>,
}
