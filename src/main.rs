use lua_rust::LuaState;
use lua_rust::value::Value;
use lua_rust::stdlib::{lua_print, lua_yield};
use lua_rust::vm::{Proto, Instruction};
use lua_rust::state::ThreadStatus;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lua = LuaState::new();

    // Proto:
    // CALL lua_print (stack[0])
    // CALL lua_yield (stack[1])
    // CALL lua_print (stack[0])
    let proto = Proto {
        instructions: vec![
            Instruction(3 | (0 << 7) | (1 << 16) | (1 << 24)), // CALL stack[0]
            Instruction(3 | (1 << 7) | (1 << 16) | (1 << 24)), // CALL stack[1] (yield)
            Instruction(3 | (0 << 7) | (1 << 16) | (1 << 24)), // CALL stack[0]
        ],
        k: vec![],
    };

    let proto_gc = {
        let mut global = lua.global.lock().unwrap();
        global.heap.allocate(proto)
    };

    lua.stack[0] = Value::RustFunction(lua_print);
    lua.stack[1] = Value::RustFunction(lua_yield);

    println!("Running Lua VM...");
    lua.execute(proto_gc).await?;

    if lua.status == ThreadStatus::Yield {
        println!("Lua VM yielded! Resuming...");
        lua.execute(proto_gc).await?;
    }

    println!("Lua VM finished");

    Ok(())
}
