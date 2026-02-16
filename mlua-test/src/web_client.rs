use mlua::prelude::*;
use reqwest::{Client, Method, header::HeaderMap};
use serde_json::Value as JsonValue;

pub struct HttpClient {
    client: Client,
}

impl LuaUserData for HttpClient {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "request_uri",
            |lua, client, (url, options): (String, Option<LuaTable>)| async move {
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

                let resp = req
                    .send()
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let status = resp.status().as_u16();
                let headers = lua.create_table()?;
                for (name, value) in resp.headers() {
                    headers.set(name.as_str(), value.to_str().unwrap_or(""))?;
                }
                let body = resp
                    .text()
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let res_table = lua.create_table()?;
                res_table.set("status", status)?;
                res_table.set("headers", headers)?;
                res_table.set("body", body)?;

                Ok((res_table, LuaValue::Nil))
            },
        );
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    // Register http module
    let http = lua.create_table()?;
    http.set(
        "new",
        lua.create_function(|_, ()| {
            Ok(HttpClient {
                client: Client::new(),
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
