use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use mlua::prelude::*;
use mlua::serde::LuaSerdeExt;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

const TOKEN_ENDPOINT: &str = "https://api.ibkr.com/v1/api/oauth/token";
const BASE_URL: &str = "https://api.ibkr.com/v1/api";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    aud: String,
    iat: i64,
    exp: i64,
    jti: String,
}

struct IbkrState {
    client_id: String,
    private_key_pem: String,
    access_token: Option<String>,
    token_expiry: Option<Instant>,
    conid_cache: HashMap<String, i64>,
    account_id: Option<String>,
}

impl IbkrState {
    async fn get_token(&mut self) -> LuaResult<String> {
        let is_valid = self.access_token.is_some()
            && self
                .token_expiry
                .is_some_and(|expiry| Instant::now() < expiry - Duration::from_secs(60));

        if is_valid {
            return Ok(self.access_token.as_ref().unwrap().clone());
        }

        let now = Utc::now().timestamp();
        let claims = Claims {
            iss: self.client_id.clone(),
            sub: self.client_id.clone(),
            aud: TOKEN_ENDPOINT.to_string(),
            iat: now,
            exp: now + 3600,
            jti: Uuid::new_v4().to_string(),
        };

        let encoding_key = EncodingKey::from_rsa_pem(self.private_key_pem.as_bytes())
            .map_err(|e| LuaError::RuntimeError(format!("Failed to create encoding key: {}", e)))?;

        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some("JWT".to_string());

        let assertion = encode(&header, &claims, &encoding_key)
            .map_err(|e| LuaError::RuntimeError(format!("Failed to sign JWT: {}", e)))?;

        let body = format!(
            "grant_type=client_credentials&client_assertion_type=urn:ietf:params:oauth:client-assertion-type:jwt-bearer&client_assertion={}",
            assertion
        );

        let resp = tokio::task::spawn_blocking(move || {
            minreq::post(TOKEN_ENDPOINT)
                .with_header("Content-Type", "application/x-www-form-urlencoded")
                .with_body(body)
                .send()
        }).await.map_err(|e| LuaError::RuntimeError(e.to_string()))?
        .map_err(|e| LuaError::RuntimeError(format!("Token request failed: {}", e)))?;

        if resp.status_code < 200 || resp.status_code >= 300 {
            let err_text = resp.as_str().unwrap_or_default();
            return Err(LuaError::RuntimeError(format!(
                "Token request returned error ({}): {}",
                resp.status_code, err_text
            )));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: u64,
        }

        let token_resp: TokenResponse = serde_json::from_str(
            resp.as_str().map_err(|e| LuaError::RuntimeError(e.to_string()))?
        ).map_err(|e| LuaError::RuntimeError(format!("Failed to parse token response: {}", e)))?;

        self.access_token = Some(token_resp.access_token.clone());
        self.token_expiry = Some(Instant::now() + Duration::from_secs(token_resp.expires_in));

        Ok(token_resp.access_token)
    }

    async fn api_request(
        &mut self,
        method: String,
        endpoint: String,
        body: Option<serde_json::Value>,
    ) -> LuaResult<serde_json::Value> {
        let token = self.get_token().await?;
        let url = format!("{}{}", BASE_URL, endpoint);

        let resp = tokio::task::spawn_blocking(move || {
            let mut req = match method.as_str() {
                "GET" => minreq::get(&url),
                "POST" => minreq::post(&url),
                "PUT" => minreq::put(&url),
                "DELETE" => minreq::delete(&url),
                _ => return Err(format!("Unsupported method: {}", method)),
            };

            req = req.with_header("Authorization", format!("Bearer {}", token));

            if let Some(b) = body {
                req = req.with_header("Content-Type", "application/json");
                req = req.with_body(serde_json::to_string(&b).unwrap_or_default());
            }

            req.send().map_err(|e| e.to_string())
        }).await.map_err(|e| LuaError::RuntimeError(e.to_string()))?
        .map_err(LuaError::RuntimeError)?;

        if resp.status_code < 200 || resp.status_code >= 300 {
            let status = resp.status_code;
            let err_text = resp.as_str().unwrap_or_default();
            return Err(LuaError::RuntimeError(format!(
                "API error ({}): {}",
                status, err_text
            )));
        }

        serde_json::from_str(
            resp.as_str().map_err(|e| LuaError::RuntimeError(e.to_string()))?
        ).map_err(|e| LuaError::RuntimeError(format!("Failed to parse API response: {}", e)))
    }

    async fn ensure_account_id(&mut self) -> LuaResult<String> {
        if let Some(id) = &self.account_id {
            return Ok(id.clone());
        }

        let accounts_val = self
            .api_request("GET".to_string(), "/portfolio/accounts".to_string(), None)
            .await?;
        let accounts = accounts_val
            .as_array()
            .ok_or_else(|| LuaError::RuntimeError("Expected array of accounts".to_string()))?;

        if accounts.is_empty() {
            return Err(LuaError::RuntimeError("No IBKR accounts found".to_string()));
        }

        let id = accounts[0]
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LuaError::RuntimeError("Account ID missing in response".to_string()))?
            .to_string();

        self.account_id = Some(id.clone());
        Ok(id)
    }

    async fn get_conid(&mut self, symbol: &str) -> LuaResult<i64> {
        if let Some(conid) = self.conid_cache.get(symbol) {
            return Ok(*conid);
        }

        let endpoint = format!(
            "/iserver/secdef/search?symbol={}&name=true&secType=STK",
            symbol
        );
        let results: serde_json::Value = self.api_request("GET".to_string(), endpoint, None).await?;

        let conid = results
            .as_array()
            .and_then(|a| a.first())
            .and_then(|o| o.get("conid"))
            .and_then(|v| v.as_i64())
            .ok_or_else(|| LuaError::RuntimeError(format!("Symbol not found: {}", symbol)))?;

        self.conid_cache.insert(symbol.to_string(), conid);
        Ok(conid)
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let client_id = match env::var("IBKR_CLIENT_ID") {
        Ok(val) => val,
        Err(_) => {
            eprintln!("Error: IBKR_CLIENT_ID environment variable is missing.");
            std::process::exit(1);
        }
    };

    let secrets_path = dirs::home_dir()
        .map(|h| h.join(".secrets"))
        .ok_or_else(|| LuaError::RuntimeError("Could not find home directory".to_string()))?;

    let mut private_key_pem = env::var("IBKR_PRIVATE_KEY").ok();

    if private_key_pem.is_none() && secrets_path.exists() {
        let content = fs::read_to_string(&secrets_path).unwrap_or_default();
        for line in content.lines() {
            if let Some(stripped) = line.strip_prefix("IBKR_PRIVATE_KEY=") {
                let val = stripped.trim_matches('"').trim_matches('\'');
                private_key_pem = Some(val.replace("\\n", "\n"));
                break;
            }
        }
    }

    if private_key_pem.is_none() {
        println!("IBKR_PRIVATE_KEY not found. Generating a new 4096-bit RSA key...");
        let mut rng = rsa::rand_core::OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 4096)
            .map_err(|e| LuaError::RuntimeError(format!("Failed to generate RSA key: {}", e)))?;

        let priv_pem = private_key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| LuaError::RuntimeError(format!("Failed to encode private key: {}", e)))?
            .to_string();

        let pub_pem = private_key
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .map_err(|e| LuaError::RuntimeError(format!("Failed to encode public key: {}", e)))?;

        let entry = format!(
            "\nIBKR_PRIVATE_KEY=\"{}\"\n",
            priv_pem.replace("\n", "\\n")
        );
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&secrets_path)
            .and_then(|mut f| {
                use std::io::Write;
                write!(f, "{}", entry)
            })
            .map_err(|e| {
                LuaError::RuntimeError(format!("Failed to save private key to .secrets: {}", e))
            })?;

        println!(
            "Successfully generated and saved private key to {:?}",
            secrets_path
        );
        println!("\n--- IBKR PUBLIC KEY REGISTRATION INSTRUCTIONS ---");
        println!("1. Log in to your IBKR Client Portal.");
        println!("2. Navigate to Settings > User Settings > API Settings.");
        println!(
            "3. Register a new OAuth 2.0 application (if not already done) using your Client ID: {}",
            client_id
        );
        println!("4. Add the following Public Key to your application configuration:");
        println!("\n{}\n", pub_pem);
        println!("5. Once registered, you can use this library to trade.");
        println!("--------------------------------------------------\n");

        private_key_pem = Some(priv_pem);
    }

    let state = Arc::new(Mutex::new(IbkrState {
        client_id,
        private_key_pem: private_key_pem.unwrap(),
        access_token: None,
        token_expiry: None,
        conid_cache: HashMap::new(),
        account_id: None,
    }));

    let ibkr = lua.create_table()?;

    let state_clone = state.clone();
    ibkr.set(
        "get_ticker",
        lua.create_async_function(move |_, symbol: String| {
            let state = state_clone.clone();
            async move {
                let mut s = state.lock().await;
                let conid = s.get_conid(&symbol).await?;
                let endpoint = format!("/iserver/marketdata/snapshot?conids={}&fields=31", conid);
                let resp: serde_json::Value = s.api_request("GET".to_string(), endpoint, None).await?;

                let last_price = resp
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|o| o.get("31"))
                    .and_then(|v| {
                        if let Some(st) = v.as_str() {
                            st.parse::<f64>().ok()
                        } else {
                            v.as_f64()
                        }
                    })
                    .ok_or_else(|| LuaError::RuntimeError("Price data not available".to_string()))?;

                Ok(last_price)
            }
        })?,
    )?;

    let state_clone = state.clone();
    let place_order = move |lua: Lua,
                            symbol: String,
                            quantity: f64,
                            order_type: String,
                            side: String,
                            price: Option<f64>| {
        let state = state_clone.clone();
        async move {
            let mut s = state.lock().await;
            let account_id = s.ensure_account_id().await?;
            let conid = s.get_conid(&symbol).await?;

            let mut order = serde_json::json!({
                "conid": conid,
                "quantity": quantity,
                "orderType": order_type,
                "side": side,
                "tif": "DAY"
            });

            if let Some(p) = price {
                order
                    .as_object_mut()
                    .unwrap()
                    .insert("price".to_string(), serde_json::json!(p));
            }

            let endpoint = format!("/iserver/account/{}/orders", account_id);
            let body = serde_json::json!([order]);
            let resp: serde_json::Value = s.api_request("POST".to_string(), endpoint, Some(body)).await?;

            lua.to_value(&resp)
        }
    };

    let pc = Arc::new(place_order);

    let pc_c = pc.clone();
    ibkr.set(
        "limit_buy",
        lua.create_async_function(move |lua, (symbol, qty, price): (String, f64, f64)| {
            let pc = pc_c.clone();
            async move {
                pc(
                    lua,
                    symbol,
                    qty,
                    "LMT".to_string(),
                    "BUY".to_string(),
                    Some(price),
                )
                .await
            }
        })?,
    )?;

    let pc_c = pc.clone();
    ibkr.set(
        "limit_sell",
        lua.create_async_function(move |lua, (symbol, qty, price): (String, f64, f64)| {
            let pc = pc_c.clone();
            async move {
                pc(
                    lua,
                    symbol,
                    qty,
                    "LMT".to_string(),
                    "SELL".to_string(),
                    Some(price),
                )
                .await
            }
        })?,
    )?;

    let pc_c = pc.clone();
    ibkr.set(
        "market_buy",
        lua.create_async_function(move |lua, (symbol, qty): (String, f64)| {
            let pc = pc_c.clone();
            async move { pc(lua, symbol, qty, "MKT".to_string(), "BUY".to_string(), None).await }
        })?,
    )?;

    let pc_c = pc.clone();
    ibkr.set(
        "market_sell",
        lua.create_async_function(move |lua, (symbol, qty): (String, f64)| {
            let pc = pc_c.clone();
            async move { pc(lua, symbol, qty, "MKT".to_string(), "SELL".to_string(), None).await }
        })?,
    )?;

    let state_clone = state.clone();
    ibkr.set(
        "cancel_order",
        lua.create_async_function(move |lua, order_id: String| {
            let state = state_clone.clone();
            async move {
                let mut s = state.lock().await;
                let account_id = s.ensure_account_id().await?;
                let endpoint = format!("/iserver/account/{}/order/{}", account_id, order_id);
                let resp: serde_json::Value =
                    s.api_request("DELETE".to_string(), endpoint, None).await?;
                lua.to_value(&resp)
            }
        })?,
    )?;

    let state_clone = state.clone();
    ibkr.set(
        "list_orders",
        lua.create_async_function(move |lua, ()| {
            let state = state_clone.clone();
            async move {
                let mut s = state.lock().await;
                let resp: serde_json::Value =
                    s.api_request("GET".to_string(), "/iserver/account/orders".to_string(), None).await?;
                lua.to_value(&resp)
            }
        })?,
    )?;

    let state_clone = state.clone();
    ibkr.set(
        "get_portfolio",
        lua.create_async_function(move |lua, ()| {
            let state = state_clone.clone();
            async move {
                let mut s = state.lock().await;
                let account_id = s.ensure_account_id().await?;
                let endpoint = format!("/portfolio/{}/positions", account_id);
                let resp: serde_json::Value = s.api_request("GET".to_string(), endpoint, None).await?;

                let positions = lua.create_table()?;
                if let Some(arr) = resp.as_array() {
                    for (i, pos) in arr.iter().enumerate() {
                        let p_table = lua.create_table()?;
                        let ticker = pos.get("ticker").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
                        let qty = pos.get("position").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        p_table.set("ticker", ticker)?;
                        p_table.set("quantity", qty)?;
                        positions.set(i + 1, p_table)?;
                    }
                }
                Ok(positions)
            }
        })?,
    )?;

    lua.globals().set("ibkr", ibkr)?;

    Ok(())
}
