use crate::state::{LuaState, ThreadStatus};
use crate::error::LuaError;
use crate::value::Value;
use futures::future::{BoxFuture, FutureExt};

pub fn lua_print(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        println!("Lua says: Hello from async Rust!");
        Ok(0)
    }.boxed()
}

pub fn lua_yield(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        state.status = ThreadStatus::Yield;
        Ok(0)
    }.boxed()
}
