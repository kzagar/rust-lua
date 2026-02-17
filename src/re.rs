use mlua::prelude::*;
use regex::Regex;

struct LuaRegex(Regex);

impl LuaUserData for LuaRegex {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("match", |lua, this, s: String| {
            if let Some(caps) = this.0.captures(&s) {
                let matches = lua.create_table()?;
                for (i, m) in caps.iter().enumerate() {
                    if let Some(m) = m {
                        matches.set(i, m.as_str())?;
                    } else {
                        matches.set(i, LuaValue::Nil)?;
                    }
                }
                // Named captures
                for name in this.0.capture_names().flatten() {
                    if let Some(m) = caps.name(name) {
                        matches.set(name, m.as_str())?;
                    }
                }
                Ok(Some(matches))
            } else {
                Ok(None)
            }
        });
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let re = lua.create_table()?;
    re.set(
        "compile",
        lua.create_function(|_, pattern: String| {
            let regex = Regex::new(&pattern)
                .map_err(|e| LuaError::RuntimeError(format!("Invalid regex: {}", e)))?;
            Ok(LuaRegex(regex))
        })?,
    )?;
    lua.globals().set("re", re)?;
    Ok(())
}
