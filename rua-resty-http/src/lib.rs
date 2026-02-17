use futures::future::{BoxFuture, FutureExt};
use rua::error::LuaError;
use rua::gc::GCTrace;
use rua::state::LuaState;
use rua::value::{LuaUserData, Table, UserData, Value};
use std::any::Any;
use std::collections::HashMap;

pub struct HttpClient {
    pub client: reqwest::Client,
}

impl GCTrace for HttpClient {
    fn trace(&self, _marked: &mut std::collections::HashSet<*const rua::gc::GcBoxHeader>) {}
}

impl LuaUserData for HttpClient {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub fn register(state: &mut LuaState) {
    let mut global = state.global.lock().unwrap();

    // Create 'resty' table if it doesn't exist
    let resty_key = Value::String(global.heap.allocate("resty".to_string()));
    let resty_table_gc = if let Value::Table(globals) = global.globals {
        if let Some(Value::Table(t)) = globals.map.get(&resty_key) {
            *t
        } else {
            let t = global.heap.allocate(Table::new());
            unsafe {
                let globals_ptr = &mut (*globals.ptr.as_ptr()).data;
                globals_ptr.map.insert(resty_key, Value::Table(t));
            }
            t
        }
    } else {
        panic!("Globals is not a table");
    };

    // Create 'http' table inside 'resty'
    let http_key = Value::String(global.heap.allocate("http".to_string()));
    let http_table_gc = global.heap.allocate(Table::new());
    unsafe {
        let resty_ptr = &mut (*resty_table_gc.ptr.as_ptr()).data;
        resty_ptr.map.insert(http_key, Value::Table(http_table_gc));
    }

    // Add 'new' to 'http' table
    let new_key = Value::String(global.heap.allocate("new".to_string()));
    unsafe {
        let http_ptr = &mut (*http_table_gc.ptr.as_ptr()).data;
        http_ptr.map.insert(new_key, Value::RustFunction(http_new));
    }
}

fn http_new(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let client = reqwest::Client::builder()
            .use_native_tls()
            .build()
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

        let mut global = state.global.lock().unwrap();

        // Create metatable for HttpClient
        let mt_gc = global.heap.allocate(Table::new());
        let index_key = Value::String(global.heap.allocate("__index".to_string()));
        let request_uri_key = Value::String(global.heap.allocate("request_uri".to_string()));

        unsafe {
            let mt = &mut (*mt_gc.ptr.as_ptr()).data;
            mt.map.insert(index_key, Value::Table(mt_gc)); // __index = mt
            mt.map
                .insert(request_uri_key, Value::RustFunction(http_request_uri));
        }

        let ud = UserData {
            data: Box::new(HttpClient { client }),
            metatable: Some(mt_gc),
        };
        let ud_gc = global.heap.allocate(ud);

        state.stack[state.top] = Value::UserData(ud_gc);
        state.top += 1;
        Ok(1)
    }
    .boxed()
}

fn http_request_uri(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        // httpc:request_uri(uri, params)
        // stack: ..., func, self (httpc), uri, params
        // nargs = 3. func is at state.top - 4.
        let nargs = 3;
        let func_idx = state.top - (nargs + 1);

        let (client, uri, params) = {
            let ud_val = state.stack[func_idx + 1];
            let uri_val = state.stack[func_idx + 2];
            let params_val = if state.top > func_idx + 3 {
                state.stack[func_idx + 3]
            } else {
                Value::Nil
            };

            let client = if let Value::UserData(ud) = ud_val {
                if let Some(c) = ud.data.as_any().downcast_ref::<HttpClient>() {
                    c.client.clone()
                } else {
                    return Err(LuaError::RuntimeError(
                        "expected HttpClient userdata".to_string(),
                    ));
                }
            } else {
                return Err(LuaError::RuntimeError(
                    "expected self as first argument".to_string(),
                ));
            };

            let uri = if let Value::String(s) = uri_val {
                s.to_string()
            } else {
                return Err(LuaError::RuntimeError(
                    "expected uri string as second argument".to_string(),
                ));
            };

            (client, uri, params_val)
        };

        let mut method = "GET".to_string();
        let mut body = None;
        let mut headers = HashMap::new();

        if let Value::Table(t) = params {
            let mut global = state.global.lock().unwrap();

            // method
            let method_key = Value::String(global.heap.allocate("method".to_string()));
            if let Some(Value::String(m)) = t.map.get(&method_key) {
                method = m.to_uppercase();
            }

            // body
            let body_key = Value::String(global.heap.allocate("body".to_string()));
            if let Some(Value::String(b)) = t.map.get(&body_key) {
                body = Some(b.to_string());
            }

            // headers
            let headers_key = Value::String(global.heap.allocate("headers".to_string()));
            if let Some(Value::Table(ht)) = t.map.get(&headers_key) {
                for (k, v) in &ht.map {
                    if let (Value::String(ks), Value::String(vs)) = (k, v) {
                        headers.insert(ks.to_string(), vs.to_string());
                    }
                }
            }
        }

        let mut req_builder = match method.as_str() {
            "GET" => client.get(&uri),
            "POST" => client.post(&uri),
            "PUT" => client.put(&uri),
            "DELETE" => client.delete(&uri),
            _ => {
                return Err(LuaError::RuntimeError(format!(
                    "unsupported method {}",
                    method
                )));
            }
        };

        if let Some(b) = body {
            req_builder = req_builder.body(b);
        }

        for (k, v) in headers {
            req_builder = req_builder.header(k, v);
        }

        let res = req_builder.send().await;
        match res {
            Ok(resp) => {
                let status = resp.status().as_u16() as i64;
                let mut res_headers = HashMap::new();
                for (name, value) in resp.headers() {
                    if let Ok(v) = value.to_str() {
                        res_headers.insert(name.to_string(), v.to_string());
                    }
                }
                let body_text = resp.text().await.unwrap_or_default();

                let mut global = state.global.lock().unwrap();
                let mut res_table = Table::new();

                let status_key = Value::String(global.heap.allocate("status".to_string()));
                res_table.map.insert(status_key, Value::Integer(status));

                let body_key = Value::String(global.heap.allocate("body".to_string()));
                res_table
                    .map
                    .insert(body_key, Value::String(global.heap.allocate(body_text)));

                let headers_key = Value::String(global.heap.allocate("headers".to_string()));
                let mut headers_table = Table::new();
                for (k, v) in res_headers {
                    let hk = Value::String(global.heap.allocate(k));
                    let hv = Value::String(global.heap.allocate(v));
                    headers_table.map.insert(hk, hv);
                }
                res_table.map.insert(
                    headers_key,
                    Value::Table(global.heap.allocate(headers_table)),
                );

                let res_table_gc = global.heap.allocate(res_table);
                state.stack[state.top] = Value::Table(res_table_gc);
                state.top += 1;
                Ok(1)
            }
            Err(e) => {
                let err_msg = e.to_string();
                let mut global = state.global.lock().unwrap();
                state.stack[state.top] = Value::Nil;
                state.stack[state.top + 1] = Value::String(global.heap.allocate(err_msg));
                state.top += 2;
                Ok(2)
            }
        }
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rua::LuaState;
    use rua::stdlib::open_libs;
    use rua::value::Value;

    #[tokio::test]
    async fn test_http_new() {
        let mut lua = LuaState::new();
        open_libs(&mut lua);
        register(&mut lua);

        // Verify resty.http.new exists
        let http_new_val = {
            let mut global = lua.global.lock().unwrap();
            if let Value::Table(globals_gc) = global.globals {
                let resty_key = Value::String(global.heap.allocate("resty".to_string()));
                let resty_table = globals_gc.map.get(&resty_key).unwrap();
                if let Value::Table(resty_gc) = resty_table {
                    let http_key = Value::String(global.heap.allocate("http".to_string()));
                    let http_table = resty_gc.map.get(&http_key).unwrap();
                    if let Value::Table(http_gc) = http_table {
                        let new_key = Value::String(global.heap.allocate("new".to_string()));
                        *http_gc.map.get(&new_key).unwrap()
                    } else {
                        panic!()
                    }
                } else {
                    panic!()
                }
            } else {
                panic!()
            }
        };

        if let Value::RustFunction(f) = http_new_val {
            let nres = f(&mut lua).await.unwrap();
            assert_eq!(nres, 1);
            if let Value::UserData(ud) = lua.stack[lua.top - 1] {
                assert!(ud.data.as_any().is::<HttpClient>());
                assert!(ud.metatable.is_some());
            } else {
                panic!("Expected userdata");
            }
        } else {
            panic!("Expected RustFunction");
        }
    }

    #[tokio::test]
    async fn test_http_request_uri_basic() {
        let mut lua = LuaState::new();
        open_libs(&mut lua);
        register(&mut lua);

        // 1. Call new() to get httpc
        let http_new_val = {
            let mut global = lua.global.lock().unwrap();
            let globals_gc = match global.globals {
                Value::Table(t) => t,
                _ => panic!(),
            };
            let resty_key = Value::String(global.heap.allocate("resty".to_string()));
            let resty_table = match globals_gc.map.get(&resty_key).unwrap() {
                Value::Table(t) => t,
                _ => panic!(),
            };
            let http_key = Value::String(global.heap.allocate("http".to_string()));
            let http_table = match resty_table.map.get(&http_key).unwrap() {
                Value::Table(t) => t,
                _ => panic!(),
            };
            let new_key = Value::String(global.heap.allocate("new".to_string()));
            match http_table.map.get(&new_key).unwrap() {
                Value::RustFunction(f) => *f,
                _ => panic!(),
            }
        };

        http_new_val(&mut lua).await.unwrap();
        let httpc = lua.stack[lua.top - 1];

        // 2. Prepare stack for request_uri call
        // stack: [..., func, self, uri, params]
        let request_uri_func = if let Value::UserData(ud) = httpc {
            let mt = ud.metatable.unwrap();
            let key = {
                let mut global = lua.global.lock().unwrap();
                Value::String(global.heap.allocate("request_uri".to_string()))
            };
            *mt.map.get(&key).unwrap()
        } else {
            panic!()
        };

        lua.stack[lua.top] = request_uri_func;
        lua.stack[lua.top + 1] = httpc;
        lua.stack[lua.top + 2] = {
            let mut global = lua.global.lock().unwrap();
            Value::String(global.heap.allocate("https://httpbin.org/get".to_string()))
        };
        lua.stack[lua.top + 3] = Value::Nil; // params
        lua.top += 4;

        if let Value::RustFunction(f) = request_uri_func {
            let nres = f(&mut lua).await.unwrap();
            assert_eq!(nres, 1);
            let res_val = lua.stack[lua.top - 1];
            if let Value::Table(res_table) = res_val {
                let status_key = {
                    let mut global = lua.global.lock().unwrap();
                    Value::String(global.heap.allocate("status".to_string()))
                };
                let status = res_table.map.get(&status_key).unwrap();
                assert_eq!(*status, Value::Integer(200));
            } else {
                panic!("Expected table result, got {:?}", res_val);
            }
        } else {
            panic!()
        }
    }
}
