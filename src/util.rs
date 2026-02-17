use hmac::{Hmac, Mac};
use mlua::prelude::*;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn register(lua: &Lua) -> LuaResult<()> {
    let crypto = lua.create_table()?;
    crypto.set(
        "hmac_sha256",
        lua.create_function(|_, (key, data): (String, String)| {
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            mac.update(data.as_bytes());
            let result = mac.finalize().into_bytes();
            Ok(hex::encode(result))
        })?,
    )?;
    lua.globals().set("crypto", crypto)?;

    let url = lua.create_table()?;
    url.set(
        "encode",
        lua.create_function(|_, s: String| Ok(urlencoding::encode(&s).to_string()))?,
    )?;

    url.set(
        "encode_query",
        lua.create_function(|_, params: LuaTable| {
            let mut pairs = Vec::new();
            for pair in params.pairs::<String, String>() {
                let (k, v) = pair?;
                pairs.push(format!(
                    "{}={}",
                    urlencoding::encode(&k),
                    urlencoding::encode(&v)
                ));
            }
            Ok(pairs.join("&"))
        })?,
    )?;
    lua.globals().set("url", url)?;

    Ok(())
}
