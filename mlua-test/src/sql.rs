use mlua::prelude::*;
use sqlx::{Column, Row, sqlite::SqlitePoolOptions, sqlite::SqliteRow};

pub struct Database {
    pool: sqlx::SqlitePool,
}

fn row_to_table(lua: &Lua, sql_row: &SqliteRow) -> LuaResult<LuaTable> {
    let table = lua.create_table()?;
    for column in sql_row.columns() {
        let name = column.name();
        // Try various types and handle nulls
        if let Ok(v) = sql_row.try_get::<String, &str>(name) {
            table.set(name, v)?;
        } else if let Ok(v) = sql_row.try_get::<i64, &str>(name) {
            table.set(name, v)?;
        } else if let Ok(v) = sql_row.try_get::<f64, &str>(name) {
            table.set(name, v)?;
        } else if let Ok(v) = sql_row.try_get::<bool, &str>(name) {
            table.set(name, v)?;
        } else {
            table.set(name, LuaValue::Nil)?;
        }
    }
    Ok(table)
}

fn bind_lua_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    val: LuaValue,
) -> LuaResult<sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>> {
    match val {
        LuaValue::String(s) => Ok(query.bind(s.to_str()?.to_string())),
        LuaValue::Integer(i) => Ok(query.bind(i)),
        LuaValue::Number(n) => Ok(query.bind(n)),
        LuaValue::Boolean(b) => Ok(query.bind(b)),
        LuaValue::Nil => Ok(query.bind(None::<String>)),
        _ => Err(LuaError::RuntimeError(format!(
            "Unsupported parameter type: {:?}",
            val
        ))),
    }
}

impl LuaUserData for Database {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "exec",
            |_, db, (sql, params): (String, Option<Vec<LuaValue>>)| async move {
                let mut query = sqlx::query(&sql);
                if let Some(params) = params {
                    for p in params {
                        query = bind_lua_value(query, p)?;
                    }
                }
                query
                    .execute(&db.pool)
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                Ok(())
            },
        );

        methods.add_async_method("close", |_, db, ()| async move {
            db.pool.close().await;
            Ok(())
        });

        methods.add_async_method(
            "rows",
            |lua, db, (sql, params): (String, Option<Vec<LuaValue>>)| async move {
                let mut query = sqlx::query(&sql);
                if let Some(params) = params {
                    for p in params {
                        query = bind_lua_value(query, p)?;
                    }
                }
                let sql_results = query
                    .fetch_all(&db.pool)
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let mut rows = Vec::new();
                for sql_row in sql_results {
                    rows.push(row_to_table(&lua, &sql_row)?);
                }

                let index = std::cell::Cell::new(0);
                let iterator = lua.create_function(move |_, ()| {
                    let curr = index.get();
                    if curr < rows.len() {
                        let row = rows[curr].clone();
                        index.set(curr + 1);
                        Ok(Some(row))
                    } else {
                        Ok(None)
                    }
                })?;

                Ok(iterator)
            },
        );

        methods.add_async_method(
            "objects",
            |lua, db, (table_name, filter): (String, Option<LuaTable>)| async move {
                let mut sql = format!("SELECT * FROM {}", table_name);
                let mut params = Vec::new();

                if let Some(filter) = filter {
                    let mut where_clauses = Vec::new();
                    let pairs = filter.pairs::<LuaValue, LuaValue>();
                    for pair in pairs {
                        let (k, v) = pair?;
                        let key = match k {
                            LuaValue::String(s) => s.to_str()?.to_string(),
                            _ => continue,
                        };

                        if let LuaValue::Table(ref t) = v {
                            if let Ok(marker) = t.get::<String>("__type") {
                                if marker == "op" {
                                    let op: String = t.get("op")?;
                                    let val: LuaValue = t.get("val")?;
                                    where_clauses.push(format!("{} {} ?", key, op));
                                    params.push(val);
                                    continue;
                                }
                            }
                        }

                        where_clauses.push(format!("{} = ?", key));
                        params.push(v);
                    }

                    if !where_clauses.is_empty() {
                        sql.push_str(" WHERE ");
                        sql.push_str(&where_clauses.join(" AND "));
                    }
                }

                let mut query = sqlx::query(&sql);
                for p in params {
                    query = bind_lua_value(query, p)?;
                }

                let sql_results = query
                    .fetch_all(&db.pool)
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let mut rows = Vec::new();
                for sql_row in sql_results {
                    rows.push(row_to_table(&lua, &sql_row)?);
                }
                Ok(rows)
            },
        );

        methods.add_async_method("add", |_, db, obj: LuaTable| async move {
            let table_name: String = obj.get("__table").map_err(|_| {
                LuaError::RuntimeError("Object does not have a __table name".into())
            })?;

            let mut keys = Vec::new();
            let mut placeholders = Vec::new();
            let mut params = Vec::new();

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
                params.push(v);
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

            let mut query = sqlx::query(&sql);
            for p in params {
                query = bind_lua_value(query, p)?;
            }

            query
                .execute(&db.pool)
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(())
        });

        methods.add_async_method("update", |_, db, obj: LuaTable| async move {
            let table_name: String = obj.get("__table").map_err(|_| {
                LuaError::RuntimeError("Object does not have a __table name".into())
            })?;
            let id: LuaValue = obj.get("id").map_err(|_| {
                LuaError::RuntimeError("Object does not have an 'id' field for update".into())
            })?;

            let mut set_clauses = Vec::new();
            let mut params = Vec::new();

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
                params.push(v);
            }

            if set_clauses.is_empty() {
                return Ok(());
            }

            let sql = format!(
                "UPDATE {} SET {} WHERE id = ?",
                table_name,
                set_clauses.join(", ")
            );
            params.push(id);

            let mut query = sqlx::query(&sql);
            for p in params {
                query = bind_lua_value(query, p)?;
            }

            query
                .execute(&db.pool)
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(())
        });

        methods.add_async_method("delete", |_, db, obj: LuaTable| async move {
            let table_name: String = obj.get("__table").map_err(|_| {
                LuaError::RuntimeError("Object does not have a __table name".into())
            })?;
            let id: LuaValue = obj.get("id").map_err(|_| {
                LuaError::RuntimeError("Object does not have an 'id' field for delete".into())
            })?;

            let sql = format!("DELETE FROM {} WHERE id = ?", table_name);
            let mut query = sqlx::query(&sql);
            query = bind_lua_value(query, id)?;

            query
                .execute(&db.pool)
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(())
        });

        methods.add_async_method(
            "find",
            |lua, db, (table_name, id): (String, LuaValue)| async move {
                let sql = format!("SELECT * FROM {} WHERE id = ? LIMIT 1", table_name);
                let mut query = sqlx::query(&sql);
                query = bind_lua_value(query, id)?;

                let row_opt = query
                    .fetch_optional(&db.pool)
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                match row_opt {
                    Some(sql_row) => {
                        let table = row_to_table(&lua, &sql_row)?;
                        table.set("__table", table_name)?;
                        Ok(Some(table))
                    }
                    None => Ok(None),
                }
            },
        );

        methods.add_async_method(
            "count",
            |_, db, (table_name, filter): (String, Option<LuaTable>)| async move {
                let mut sql = format!("SELECT COUNT(*) FROM {}", table_name);
                let mut params = Vec::new();

                if let Some(filter) = filter {
                    let mut where_clauses = Vec::new();
                    let pairs = filter.pairs::<LuaValue, LuaValue>();
                    for pair in pairs {
                        let (k, v) = pair?;
                        let key = match k {
                            LuaValue::String(s) => s.to_str()?.to_string(),
                            _ => continue,
                        };

                        if let LuaValue::Table(ref t) = v {
                            if let Ok(marker) = t.get::<String>("__type") {
                                if marker == "op" {
                                    let op: String = t.get("op")?;
                                    let val: LuaValue = t.get("val")?;
                                    where_clauses.push(format!("{} {} ?", key, op));
                                    params.push(val);
                                    continue;
                                }
                            }
                        }

                        where_clauses.push(format!("{} = ?", key));
                        params.push(v);
                    }

                    if !where_clauses.is_empty() {
                        sql.push_str(" WHERE ");
                        sql.push_str(&where_clauses.join(" AND "));
                    }
                }

                let mut query = sqlx::query(&sql);
                for p in params {
                    query = bind_lua_value(query, p)?;
                }

                let row = query
                    .fetch_one(&db.pool)
                    .await
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

                let count: i64 = row.get(0);
                Ok(count)
            },
        );
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    // Register sqlite3 module
    let sqlite3 = lua.create_table()?;
    sqlite3.set(
        "open",
        lua.create_async_function(|_, path: String| async move {
            use std::str::FromStr;
            let options =
                sqlx::sqlite::SqliteConnectOptions::from_str(&format!("sqlite://{}", path))
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))?
                    .create_if_missing(true);

            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(options)
                .await
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(Database { pool })
        })?,
    )?;
    lua.globals().set("sqlite3", sqlite3)?;

    // Register ORM helpers as globals as requested
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

    // Additional operators for a premium feel
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

