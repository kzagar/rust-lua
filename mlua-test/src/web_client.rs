use mlua::prelude::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

pub struct HttpClient {
    insecure: bool,
}

impl LuaUserData for HttpClient {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "request_uri",
            |lua, client, (url, options): (String, Option<LuaTable>)| {
                let insecure = client.insecure;
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
                        let mut req = match method.as_str() {
                            "GET" => minreq::get(&url),
                            "POST" => minreq::post(&url),
                            "PUT" => minreq::put(&url),
                            "DELETE" => minreq::delete(&url),
                            "PATCH" => minreq::patch(&url),
                            "HEAD" => minreq::head(&url),
                            _ => return Err(format!("Unsupported method: {}", method)),
                        };

                        if insecure {
                            // Insecure not easily supported in minreq 2.x without custom proxy/handling
                        }

                        for (k, v) in headers {
                            req = req.with_header(k, v);
                        }

                        if let Some(b) = body {
                            req = req.with_body(b);
                        }

                        req.send().map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                    .map_err(LuaError::RuntimeError)?;

                    let status = res.status_code;
                    let res_headers = lua_ref.create_table()?;
                    for (name, value) in &res.headers {
                        res_headers.set(name.as_str(), value.as_str())?;
                    }
                    let res_body = res
                        .as_str()
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                        .to_string();

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
            if let Some(opts) = options {
                insecure = opts.get::<bool>("insecure").unwrap_or(false);
            }
            Ok(HttpClient { insecure })
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
