use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct GcpCredentials {
    project_id: String,
    private_key: String,
    client_email: String,
}

#[derive(Debug, Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: u64,
    iat: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    // expires_in: u64,
}

#[derive(Debug, Serialize)]
struct LogEntry {
    log_name: String,
    resource: MonitoredResource,
    entries: Vec<Entry>,
}

#[derive(Debug, Serialize)]
struct MonitoredResource {
    #[serde(rename = "type")]
    resource_type: String,
    labels: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct Entry {
    severity: String,
    text_payload: String,
    timestamp: String,
}

pub struct GcpLoggerClient {
    tx: mpsc::Sender<(String, String)>,
}

impl GcpLoggerClient {
    pub fn new() -> Option<Self> {
        let creds = load_credentials()?;
        let (tx, rx) = mpsc::channel::<(String, String)>();

        thread::spawn(move || {
            let mut worker = GcpWorker::new(creds);
            while let Ok((severity, message)) = rx.recv() {
                if let Err(e) = worker.log(&severity, &message) {
                    eprintln!("Failed to send log to GCP: {}", e);
                }
            }
        });

        Some(GcpLoggerClient { tx })
    }

    pub fn log(&self, severity: &str, message: &str) {
        let _ = self.tx.send((severity.to_string(), message.to_string()));
    }
}

struct GcpWorker {
    creds: GcpCredentials,
    access_token: Option<String>,
    token_expiry: u64,
}

impl GcpWorker {
    fn new(creds: GcpCredentials) -> Self {
        Self {
            creds,
            access_token: None,
            token_expiry: 0,
        }
    }

    fn get_token(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let (Some(token), true) = (&self.access_token, now < self.token_expiry - 60) {
            return Ok(token.clone());
        }

        let claims = Claims {
            iss: self.creds.client_email.clone(),
            scope: "https://www.googleapis.com/auth/logging.write".to_string(),
            aud: "https://oauth2.googleapis.com/token".to_string(),
            iat: now,
            exp: now + 3600,
        };

        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(self.creds.private_key.as_bytes())?;
        let jwt = jsonwebtoken::encode(&header, &claims, &key)?;

        let resp: TokenResponse = ureq::post("https://oauth2.googleapis.com/token")
            .send_form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])?
            .into_json()?;

        self.access_token = Some(resp.access_token.clone());
        self.token_expiry = now + 3600;

        Ok(resp.access_token)
    }

    fn log(&mut self, severity: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token()?;

        let now = chrono::Utc::now();
        let timestamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let mut labels = std::collections::HashMap::new();
        labels.insert("project_id".to_string(), self.creds.project_id.clone());

        let payload = LogEntry {
            log_name: format!("projects/{}/logs/mlua-test", self.creds.project_id),
            resource: MonitoredResource {
                resource_type: "global".to_string(),
                labels,
            },
            entries: vec![Entry {
                severity: severity.to_string(),
                text_payload: message.to_string(),
                timestamp,
            }],
        };

        let _resp = ureq::post("https://logging.googleapis.com/v2/entries:write")
            .set("Authorization", &format!("Bearer {}", token))
            .send_json(payload)?;

        Ok(())
    }
}

fn load_credentials() -> Option<GcpCredentials> {
    let path = if let Ok(env_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        PathBuf::from(env_path)
    } else {
        let mut p = dirs::home_dir()?;
        p.push(".secrets");
        p.push("service-account.json");
        p
    };

    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}
