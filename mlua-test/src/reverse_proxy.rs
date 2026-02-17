use crate::types::{AppState, ReverseProxyInfo};
use mlua::prelude::*;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

pub struct ProxyBuilder {
    state: Arc<Mutex<AppState>>,
    index: usize,
}

impl LuaUserData for ProxyBuilder {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("require_auth", |_, builder, domain: String| {
            let mut state = builder.state.lock().unwrap();
            if let Some(proxy) = state.reverse_proxies.get_mut(builder.index) {
                proxy.domain = Some(domain);
            }
            Ok(builder.clone())
        });

        methods.add_method("auth_callback", |lua, builder, func: LuaFunction| {
            let mut state = builder.state.lock().unwrap();
            let callback_key = lua.create_registry_value(func)?;
            if let Some(proxy) = state.reverse_proxies.get_mut(builder.index) {
                proxy.auth_callback = Some(Arc::new(callback_key));
            }
            Ok(builder.clone())
        });
    }
}

impl Clone for ProxyBuilder {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            index: self.index,
        }
    }
}

pub struct DomainManager {
    name: String,
    db_conn: Arc<Mutex<Connection>>,
}

impl LuaUserData for DomainManager {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("add_user", |_, manager, email: String| {
            let conn = manager.db_conn.lock().unwrap();
            conn.execute(
                "INSERT OR IGNORE INTO authorized_users (domain, email) VALUES (?, ?)",
                params![manager.name, email],
            )
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(())
        });

        methods.add_method("remove_user", |_, manager, email: String| {
            let conn = manager.db_conn.lock().unwrap();
            conn.execute(
                "DELETE FROM authorized_users WHERE domain = ? AND email = ?",
                params![manager.name, email],
            )
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(())
        });
    }
}

pub fn register(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    let reverse_proxy = lua.create_table()?;

    let state_clone = app_state.clone();
    reverse_proxy.set(
        "add",
        lua.create_function(
            move |_, (host, path_prefix, remote_base): (String, String, String)| {
                let mut state = state_clone.lock().unwrap();
                let index = state.reverse_proxies.len();
                state.reverse_proxies.push(ReverseProxyInfo {
                    host,
                    path_prefix,
                    remote_base,
                    domain: None,
                    auth_callback: None,
                });
                Ok(ProxyBuilder {
                    state: state_clone.clone(),
                    index,
                })
            },
        )?,
    )?;

    // Setup database for domain management
    let db_conn = Connection::open("server.db").map_err(|e| LuaError::RuntimeError(e.to_string()))?;
    db_conn
        .execute(
            "CREATE TABLE IF NOT EXISTS authorized_users (
                domain TEXT,
                email TEXT,
                PRIMARY KEY (domain, email)
            )",
            [],
        )
        .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
    let db_conn = Arc::new(Mutex::new(db_conn));

    let db_conn_clone = db_conn.clone();
    reverse_proxy.set(
        "domain",
        lua.create_function(move |_, name: String| {
            Ok(DomainManager {
                name,
                db_conn: db_conn_clone.clone(),
            })
        })?,
    )?;

    lua.globals().set("reverse_proxy", reverse_proxy.clone())?;

    // Also register domain as a global function as requested
    let domain_func: LuaFunction = reverse_proxy.get("domain")?;
    lua.globals().set("domain", domain_func)?;

    Ok(())
}
