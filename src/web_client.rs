use mlua::prelude::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

pub struct HttpClient {
    #[allow(dead_code)]
    insecure: bool,
    agent: Arc<ureq::Agent>,
}

impl LuaUserData for HttpClient {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "request_uri",
            |lua, client, (url, options): (String, Option<LuaTable>)| {
                let agent = client.agent.clone();
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

                    let res = tokio::task::spawn_blocking(move || {
                        let mut request = agent.request(&method, &url);

                        for (k, v) in headers {
                            request = request.set(&k, &v);
                        }

                        let response = if let Some(b) = body {
                            request.send_string(&b)
                        } else {
                            request.call()
                        };

                        response.map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                    .map_err(LuaError::RuntimeError)?;

                    let status = res.status();
                    let res_headers = lua_ref.create_table()?;
                    for name in res.headers_names() {
                        if let Some(value) = res.header(&name) {
                            res_headers.set(name, value)?;
                        }
                    }
                    let res_body = res
                        .into_string()
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                    let res_table = lua_ref.create_table()?;
                    res_table.set("status", status)?;
                    res_table.set("headers", res_headers)?;
                    res_table.set("body", res_body)?;

                    Ok((res_table, LuaValue::Nil))
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
            if let Some(opts) = options {
                insecure = opts.get::<bool>("insecure").unwrap_or(false);
                user_agent = opts.get::<Option<String>>("user_agent")?;
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
