use mlua::prelude::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

pub struct HttpClient {
    #[allow(dead_code)]
    insecure: bool,
    agent: Arc<ureq::Agent>,
    max_retries: u32,
    retry_delays: Vec<u64>,
}

impl LuaUserData for HttpClient {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "request_uri",
            |lua, client, (url, options): (String, Option<LuaTable>)| {
                let agent = client.agent.clone();
                let max_retries = client.max_retries;
                let retry_delays = client.retry_delays.clone();
                let lua_ref = lua.clone();
                async move {
                    let mut method = "GET".to_string();
                    let mut body = None;
                    let mut headers = HashMap::new();

                    if let Some(opts) = options {
                        if let Some(m) = opts.get::<Option<String>>("method")? {
                            method = m.to_uppercase();
                        }
                        body = opts.get::<Option<String>>("body")?;
                        if let Some(h_table) = opts.get::<Option<LuaTable>>("headers")? {
                            for pair in h_table.pairs::<String, String>() {
                                let (k, v) = pair?;
                                headers.insert(k, v);
                            }
                        }
                    }

                    let mut last_error = "Unknown error".to_string();

                    for attempt in 0..=max_retries {
                        if attempt > 0 {
                            let delay = retry_delays
                                .get((attempt - 1) as usize)
                                .cloned()
                                .unwrap_or(30);
                            tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                        }

                        let method_inner = method.clone();
                        let url_inner = url.clone();
                        let headers_inner = headers.clone();
                        let body_inner = body.clone();
                        let agent_inner = agent.clone();

                        let res_result = tokio::task::spawn_blocking(move || {
                            let mut request = agent_inner.request(&method_inner, &url_inner);

                            for (k, v) in headers_inner {
                                request = request.set(&k, &v);
                            }

                            if let Some(b) = body_inner {
                                request.send_string(&b)
                            } else {
                                request.call()
                            }
                        })
                        .await;

                        let response = match res_result {
                            Ok(Ok(res)) => res,
                            Ok(Err(e)) => {
                                last_error = e.to_string();
                                match e {
                                    ureq::Error::Status(code, res) => {
                                        if code >= 500 && attempt < max_retries {
                                            continue;
                                        }
                                        // Return response for 4xx or if no more retries for 5xx
                                        let status = code;
                                        let res_headers = lua_ref.create_table()?;
                                        for name in res.headers_names() {
                                            if let Some(value) = res.header(&name) {
                                                res_headers.set(name, value)?;
                                            }
                                        }
                                        let mut bytes = Vec::new();
                                        res.into_reader()
                                            .read_to_end(&mut bytes)
                                            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                                        let res_body = lua_ref.create_string(&bytes)?;

                                        let res_table = lua_ref.create_table()?;
                                        res_table.set("status", status)?;
                                        res_table.set("headers", res_headers)?;
                                        res_table.set("body", res_body)?;

                                        return Ok((LuaValue::Table(res_table), LuaValue::Nil));
                                    }
                                    _ => {
                                        if attempt < max_retries {
                                            continue;
                                        }
                                        return Ok((
                                            LuaValue::Nil,
                                            LuaValue::String(lua_ref.create_string(&last_error)?),
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                last_error = e.to_string();
                                if attempt < max_retries {
                                    continue;
                                }
                                return Ok((
                                    LuaValue::Nil,
                                    LuaValue::String(lua_ref.create_string(&last_error)?),
                                ));
                            }
                        };

                        let status = response.status();
                        // ureq::Error::Status covers >= 400. So if we are here, it's < 400.
                        // But wait, ureq 2.x `call()` returns `Result<Response, Error>`.
                        // Success is 2xx or 3xx (if following redirects).

                        let res_headers = lua_ref.create_table()?;
                        for name in response.headers_names() {
                            if let Some(value) = response.header(&name) {
                                res_headers.set(name, value)?;
                            }
                        }
                        let mut bytes = Vec::new();
                        response
                            .into_reader()
                            .read_to_end(&mut bytes)
                            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                        let res_body = lua_ref.create_string(&bytes)?;

                        let res_table = lua_ref.create_table()?;
                        res_table.set("status", status)?;
                        res_table.set("headers", res_headers)?;
                        res_table.set("body", res_body)?;

                        return Ok((LuaValue::Table(res_table), LuaValue::Nil));
                    }

                    Ok((
                        LuaValue::Nil,
                        LuaValue::String(lua_ref.create_string(&last_error)?),
                    ))
                }
            },
        );
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    // Register http module
    let http = lua.create_table()?;
    http.set(
        "new",
        lua.create_function(|_, options: Option<LuaTable>| {
            let mut insecure = false;
            let mut user_agent = None;
            let mut max_retries = 3;
            let mut retry_delays = vec![1, 5, 30];

            if let Some(opts) = options {
                insecure = opts.get::<bool>("insecure").unwrap_or(false);
                user_agent = opts.get::<Option<String>>("user_agent")?;
                if let Some(mr) = opts.get::<Option<u32>>("max_retries")? {
                    max_retries = mr;
                }
                if let Some(rd) = opts.get::<Option<Vec<u64>>>("retry_delays")? {
                    retry_delays = rd;
                }
            }

            let mut agent_builder = ureq::AgentBuilder::new();
            if insecure {
                let connector = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true)
                    .build()
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                agent_builder = agent_builder.tls_connector(Arc::new(connector));
            }
            if let Some(ua) = user_agent {
                agent_builder = agent_builder.user_agent(&ua);
            }

            Ok(HttpClient {
                insecure,
                agent: Arc::new(agent_builder.build()),
                max_retries,
                retry_delays,
            })
        })?,
    )?;
    lua.globals().set("http", http)?;

    // Register json module
    let json = lua.create_table()?;
    json.set(
        "encode",
        lua.create_function(|lua, value: LuaValue| {
            let json_val: JsonValue = lua.from_value(value)?;
            serde_json::to_string(&json_val).map_err(|e| LuaError::RuntimeError(e.to_string()))
        })?,
    )?;
    json.set(
        "decode",
        lua.create_function(|lua, s: String| {
            let json_val: JsonValue =
                serde_json::from_str(&s).map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            lua.to_value(&json_val)
        })?,
    )?;
    lua.globals().set("json", json)?;

    Ok(())
}
