use mlua::prelude::*;
use rusqlite::{Connection, ToSql, params};
use std::sync::{Arc, Mutex};

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

type RowData = Vec<(String, RusqliteValue)>;

#[derive(Clone)]
enum RusqliteValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

fn collect_row(row: &rusqlite::Row, column_names: &[String]) -> RowData {
    let mut data = Vec::new();
    for (i, name) in column_names.iter().enumerate() {
        let val = match row.get_ref(i).unwrap() {
            rusqlite::types::ValueRef::Null => RusqliteValue::Null,
            rusqlite::types::ValueRef::Integer(i) => RusqliteValue::Integer(i),
            rusqlite::types::ValueRef::Real(f) => RusqliteValue::Real(f),
            rusqlite::types::ValueRef::Text(s) => {
                RusqliteValue::Text(std::str::from_utf8(s).unwrap_or("").to_string())
            }
            rusqlite::types::ValueRef::Blob(b) => RusqliteValue::Blob(b.to_vec()),
        };
        data.push((name.clone(), val));
    }
    data
}

fn row_data_to_table(lua: &Lua, data: RowData) -> LuaResult<LuaTable> {
    let table = lua.create_table()?;
    for (name, val) in data {
        match val {
            RusqliteValue::Null => table.set(name, LuaValue::Nil)?,
            RusqliteValue::Integer(i) => table.set(name, i)?,
            RusqliteValue::Real(f) => table.set(name, f)?,
            RusqliteValue::Text(s) => table.set(name, s)?,
            RusqliteValue::Blob(b) => table.set(name, lua.create_string(&b)?)?,
        }
    }
    Ok(table)
}

fn lua_to_rusqlite(val: LuaValue) -> LuaResult<Box<dyn ToSql + Send>> {
    match val {
        LuaValue::String(s) => Ok(Box::new(s.to_str()?.to_string())),
        LuaValue::Integer(i) => Ok(Box::new(i)),
        LuaValue::Number(n) => Ok(Box::new(n)),
        LuaValue::Boolean(b) => Ok(Box::new(b)),
        LuaValue::Nil => Ok(Box::new(rusqlite::types::Null)),
        _ => Err(LuaError::RuntimeError(format!(
            "Unsupported parameter type: {:?}",
            val
        ))),
    }
}

async fn fetch_all(
    conn: Arc<Mutex<Connection>>,
    sql: String,
    params_lua: Option<Vec<LuaValue>>,
) -> LuaResult<Vec<RowData>> {
    let mut p = Vec::new();
    if let Some(params_lua) = params_lua {
        for val in params_lua {
            p.push(lua_to_rusqlite(val)?);
        }
    }

    tokio::task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        let p_refs: Vec<&dyn ToSql> = p.iter().map(|x| x.as_ref() as &dyn ToSql).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let rows = stmt
            .query_map(p_refs.as_slice(), |row| Ok(collect_row(row, &column_names)))
            .map_err(|e| e.to_string())?;

        let mut collected = Vec::new();
        for row in rows {
            collected.push(row.map_err(|e| e.to_string())?);
        }
        Ok::<Vec<RowData>, String>(collected)
    })
    .await
    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
    .map_err(LuaError::RuntimeError)
}

impl LuaUserData for Database {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "exec",
            |_, db, (sql, params_lua): (String, Option<Vec<LuaValue>>)| {
                let conn = db.conn.clone();
                async move {
                    let mut p = Vec::new();
                    if let Some(params_lua) = params_lua {
                        for val in params_lua {
                            p.push(lua_to_rusqlite(val)?);
                        }
                    }

                    tokio::task::spawn_blocking(move || {
                        let conn = conn.lock().unwrap();
                        let p_refs: Vec<&dyn ToSql> =
                            p.iter().map(|x| x.as_ref() as &dyn ToSql).collect();
                        conn.execute(&sql, p_refs.as_slice())
                            .map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                    .map_err(LuaError::RuntimeError)?;
                    Ok(())
                }
            },
        );

        methods.add_async_method("close", |_, _db, ()| async move { Ok(()) });

        methods.add_async_method(
            "rows",
            |lua, db, (sql, params_lua): (String, Option<Vec<LuaValue>>)| {
                let conn = db.conn.clone();
                let lua_ref = lua.clone();
                async move {
                    let results = fetch_all(conn, sql, params_lua).await?;

                    let mut lua_rows = Vec::new();
                    for data in results {
                        lua_rows.push(row_data_to_table(&lua_ref, data)?);
                    }

                    let index = std::cell::Cell::new(0);
                    let iterator = lua_ref.create_function(move |_, ()| {
                        let curr = index.get();
                        if curr < lua_rows.len() {
                            let row = lua_rows[curr].clone();
                            index.set(curr + 1);
                            Ok(Some(row))
                        } else {
                            Ok(None)
                        }
                    })?;

                    Ok(iterator)
                }
            },
        );

        methods.add_async_method(
            "objects",
            |lua, db, (table_name, filter): (String, Option<LuaTable>)| {
                let conn = db.conn.clone();
                let lua_ref = lua.clone();
                async move {
                    let mut sql = format!("SELECT * FROM {}", table_name);
                    let mut params_lua = Vec::new();

                    if let Some(filter) = filter {
                        let mut where_clauses = Vec::new();
                        let pairs = filter.pairs::<LuaValue, LuaValue>();
                        for pair in pairs {
                            let (k, v) = pair?;
                            let key = match k {
                                LuaValue::String(s) => s.to_str()?.to_string(),
                                _ => continue,
                            };

                            if let LuaValue::Table(ref t) = v
                                && let Ok(marker) = t.get::<String>("__type")
                                && marker == "op"
                            {
                                let op: String = t.get("op")?;
                                let val: LuaValue = t.get("val")?;
                                where_clauses.push(format!("{} {} ?", key, op));
                                params_lua.push(val);
                                continue;
                            }

                            where_clauses.push(format!("{} = ?", key));
                            params_lua.push(v);
                        }

                        if !where_clauses.is_empty() {
                            sql.push_str(" WHERE ");
                            sql.push_str(&where_clauses.join(" AND "));
                        }
                    }

                    let results = fetch_all(conn, sql, Some(params_lua)).await?;
                    let mut rows = Vec::new();
                    for data in results {
                        rows.push(row_data_to_table(&lua_ref, data)?);
                    }
                    Ok(rows)
                }
            },
        );

        methods.add_async_method("add", |_, db, obj: LuaTable| {
            let conn = db.conn.clone();
            async move {
                let table_name: String = obj.get("__table").map_err(|_| {
                    LuaError::RuntimeError("Object does not have a __table name".into())
                })?;

                let mut keys = Vec::new();
                let mut placeholders = Vec::new();
                let mut params_lua = Vec::new();

                let pairs = obj.pairs::<LuaValue, LuaValue>();
                for pair in pairs {
                    let (k, v) = pair?;
                    let key = match k {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => continue,
                    };
                    if key.starts_with("__") {
                        continue; // Skip internal fields
                    }
                    keys.push(key);
                    placeholders.push("?");
                    params_lua.push(v);
                }

                if keys.is_empty() {
                    return Err(LuaError::RuntimeError("No fields to insert".into()));
                }

                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    table_name,
                    keys.join(", "),
                    placeholders.join(", ")
                );

                let mut p = Vec::new();
                for val in params_lua {
                    p.push(lua_to_rusqlite(val)?);
                }

                tokio::task::spawn_blocking(move || {
                    let conn = conn.lock().unwrap();
                    let p_refs: Vec<&dyn ToSql> =
                        p.iter().map(|x| x.as_ref() as &dyn ToSql).collect();
                    conn.execute(&sql, p_refs.as_slice())
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                .map_err(LuaError::RuntimeError)?;
                Ok(())
            }
        });

        methods.add_async_method("update", |_, db, obj: LuaTable| {
            let conn = db.conn.clone();
            async move {
                let table_name: String = obj.get("__table").map_err(|_| {
                    LuaError::RuntimeError("Object does not have a __table name".into())
                })?;
                let id: LuaValue = obj.get("id").map_err(|_| {
                    LuaError::RuntimeError("Object does not have an 'id' field for update".into())
                })?;

                let mut set_clauses = Vec::new();
                let mut params_lua = Vec::new();

                let pairs = obj.pairs::<LuaValue, LuaValue>();
                for pair in pairs {
                    let (k, v) = pair?;
                    let key = match k {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => continue,
                    };
                    if key.starts_with("__") || key == "id" {
                        continue; // Skip internal fields and primary key
                    }
                    set_clauses.push(format!("{} = ?", key));
                    params_lua.push(v);
                }

                if set_clauses.is_empty() {
                    return Ok(());
                }

                let sql = format!(
                    "UPDATE {} SET {} WHERE id = ?",
                    table_name,
                    set_clauses.join(", ")
                );
                params_lua.push(id);

                let mut p = Vec::new();
                for val in params_lua {
                    p.push(lua_to_rusqlite(val)?);
                }

                tokio::task::spawn_blocking(move || {
                    let conn = conn.lock().unwrap();
                    let p_refs: Vec<&dyn ToSql> =
                        p.iter().map(|x| x.as_ref() as &dyn ToSql).collect();
                    conn.execute(&sql, p_refs.as_slice())
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                .map_err(LuaError::RuntimeError)?;
                Ok(())
            }
        });

        methods.add_async_method("delete", |_, db, obj: LuaTable| {
            let conn = db.conn.clone();
            async move {
                let table_name: String = obj.get("__table").map_err(|_| {
                    LuaError::RuntimeError("Object does not have a __table name".into())
                })?;
                let id: LuaValue = obj.get("id").map_err(|_| {
                    LuaError::RuntimeError("Object does not have an 'id' field for delete".into())
                })?;

                let sql = format!("DELETE FROM {} WHERE id = ?", table_name);
                let p = lua_to_rusqlite(id)?;

                tokio::task::spawn_blocking(move || {
                    let conn = conn.lock().unwrap();
                    conn.execute(&sql, params![p.as_ref()])
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                .map_err(LuaError::RuntimeError)?;
                Ok(())
            }
        });

        methods.add_async_method("find", |lua, db, (table_name, id): (String, LuaValue)| {
            let conn = db.conn.clone();
            let lua_ref = lua.clone();
            async move {
                let sql = format!("SELECT * FROM {} WHERE id = ? LIMIT 1", table_name);
                let results = fetch_all(conn, sql, Some(vec![id])).await?;

                match results.into_iter().next() {
                    Some(data) => {
                        let table = row_data_to_table(&lua_ref, data)?;
                        table.set("__table", table_name)?;
                        Ok(Some(table))
                    }
                    None => Ok(None),
                }
            }
        });

        methods.add_async_method(
            "count",
            |_, db, (table_name, filter): (String, Option<LuaTable>)| {
                let conn = db.conn.clone();
                async move {
                    let mut sql = format!("SELECT COUNT(*) FROM {}", table_name);
                    let mut params_lua = Vec::new();

                    if let Some(filter) = filter {
                        let mut where_clauses = Vec::new();
                        let pairs = filter.pairs::<LuaValue, LuaValue>();
                        for pair in pairs {
                            let (k, v) = pair?;
                            let key = match k {
                                LuaValue::String(s) => s.to_str()?.to_string(),
                                _ => continue,
                            };

                            if let LuaValue::Table(ref t) = v
                                && let Ok(marker) = t.get::<String>("__type")
                                && marker == "op"
                            {
                                let op: String = t.get("op")?;
                                let val: LuaValue = t.get("val")?;
                                where_clauses.push(format!("{} {} ?", key, op));
                                params_lua.push(val);
                                continue;
                            }

                            where_clauses.push(format!("{} = ?", key));
                            params_lua.push(v);
                        }

                        if !where_clauses.is_empty() {
                            sql.push_str(" WHERE ");
                            sql.push_str(&where_clauses.join(" AND "));
                        }
                    }

                    let results = fetch_all(conn, sql, Some(params_lua)).await?;
                    if let Some(row) = results.into_iter().next()
                        && let Some((_, RusqliteValue::Integer(count))) = row.into_iter().next()
                    {
                        return Ok(count);
                    }
                    Ok(0)
                }
            },
        );
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let sqlite3 = lua.create_table()?;
    sqlite3.set(
        "open",
        lua.create_async_function(|_, path: String| async move {
            let conn = tokio::task::spawn_blocking(move || {
                Connection::open(path).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .map_err(LuaError::RuntimeError)?;

            Ok(Database {
                conn: Arc::new(Mutex::new(conn)),
            })
        })?,
    )?;
    lua.globals().set("sqlite3", sqlite3)?;

    let like = lua.create_function(|lua, pattern: String| {
        let t = lua.create_table()?;
        t.set("__type", "op")?;
        t.set("op", "LIKE")?;
        t.set("val", pattern)?;
        Ok(t)
    })?;
    lua.globals().set("like", like)?;

    let new_object =
        lua.create_function(|lua, (table_name, data): (String, Option<LuaValue>)| {
            let t = match data {
                Some(LuaValue::Table(t)) => t,
                _ => lua.create_table()?,
            };
            t.set("__table", table_name)?;
            Ok(t)
        })?;
    lua.globals().set("new_object", new_object)?;

    let ops = [
        ("gt", ">"),
        ("lt", "<"),
        ("ge", ">="),
        ("le", "<="),
        ("ne", "!="),
    ];
    for (name, op) in ops {
        let func = lua.create_function(move |lua, val: LuaValue| {
            let t = lua.create_table()?;
            t.set("__type", "op")?;
            t.set("op", op)?;
            t.set("val", val)?;
            Ok(t)
        })?;
        lua.globals().set(name, func)?;
    }

    Ok(())
}
