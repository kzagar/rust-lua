use mlua::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct FileObject {
    pub id: Option<String>,
    pub name: String,
    pub mime_type: Option<String>,
    pub path: Option<String>,
    pub blob: Option<Vec<u8>>,
    pub downloader: Option<Arc<LuaRegistryKey>>,
}

impl LuaUserData for FileObject {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("mime_type", |_, this| Ok(this.mime_type.clone()));
        fields.add_field_method_get("path", |_, this| Ok(this.path.clone()));
        fields.add_field_method_get("id", |_, this| Ok(this.id.clone()));
        fields.add_field_method_set("id", |_, this, id: Option<String>| {
            this.id = id;
            Ok(())
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("mime", |_, this, m: String| {
            this.mime_type = Some(m);
            Ok(this.clone())
        });

        methods.add_method_mut("path", |_, this, p: String| {
            this.path = Some(p);
            this.blob = None;
            if this.mime_type.is_none() {
                this.mime_type = Some(detect_mime(&this.name));
            }
            Ok(this.clone())
        });

        methods.add_method_mut("blob", |_, this, b: LuaValue| {
            if let LuaValue::String(s) = b {
                this.blob = Some(s.as_bytes().to_vec());
                this.path = None;
                if this.mime_type.is_none() {
                    this.mime_type = Some(detect_mime(&this.name));
                }
                Ok(this.clone())
            } else {
                Err(LuaError::RuntimeError("expected string for blob".into()))
            }
        });

        methods.add_method_mut("set_downloader", |lua, this, func: LuaFunction| {
            this.downloader = Some(Arc::new(lua.create_registry_value(func)?));
            Ok(this.clone())
        });

        methods.add_async_method("get_blob", |lua, this, ()| async move {
            if let Some(ref b) = this.blob {
                return lua.create_string(b);
            }
            if let Some(ref p) = this.path {
                let data = fs::read(p).map_err(|e| LuaError::RuntimeError(e.to_string()))?;
                return lua.create_string(&data);
            }
            if let Some(ref reg_key) = this.downloader {
                let func: LuaFunction = lua.registry_value(reg_key.as_ref())?;
                let res: LuaValue = func.call_async(this.clone()).await?;
                if let LuaValue::String(s) = res {
                    return Ok(s);
                } else {
                    return Err(LuaError::RuntimeError(
                        "Downloader must return a string (blob)".into(),
                    ));
                }
            }
            Err(LuaError::RuntimeError("No data in file object".into()))
        });
    }
}

pub fn detect_mime(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match ext.to_lowercase().as_str() {
        "json" => "application/json".to_string(),
        "txt" => "text/plain".to_string(),
        "html" => "text/html".to_string(),
        "pdf" => "application/pdf".to_string(),
        "zip" => "application/zip".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let file_mod = lua.create_table()?;
    file_mod.set(
        "new",
        lua.create_function(|_, name: String| {
            Ok(FileObject {
                id: None,
                name,
                mime_type: None,
                path: None,
                blob: None,
                downloader: None,
            })
        })?,
    )?;
    lua.globals().set("file", file_mod)?;
    Ok(())
}
