use mlua::RegistryKey;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tokio::sync::oneshot as tokio_oneshot;

pub struct RestRequest {
    pub callback_id: usize,
    pub params: HashMap<String, String>,
    pub response_tx: tokio_oneshot::Sender<Result<JsonValue, String>>,
}

pub enum EngineRequest {
    Rest(RestRequest),
    Cron(usize),
    TelegramUpdate(JsonValue),
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

pub struct AppState {
    pub routes: Vec<RestRouteInfo>,
    pub static_routes: Vec<(String, String)>,
    pub cron_jobs: Vec<CronJobInfo>,
    pub telegram_handler: Option<RegistryKey>,
    pub config: Option<ServerConfig>,
    pub gmail_state: Option<std::sync::Arc<crate::gmail::GmailState>>,
    pub drive_state: Option<std::sync::Arc<crate::gmail::GmailState>>,
}
