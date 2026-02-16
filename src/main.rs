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
            Instruction(67 | (0 << 7) | (1 << 24) | (1 << 15)), // CALL stack[0], B=1, C=1
            Instruction(67 | (1 << 7) | (1 << 24) | (1 << 15)), // CALL stack[1] (yield)
            Instruction(67 | (0 << 7) | (1 << 24) | (1 << 15)), // CALL stack[0]
        ],
        k: vec![],
        upvalues: vec![],
        protos: vec![],
        numparams: 0,
        is_vararg: false,
        maxstacksize: 2,
    };

    let closure_gc = {
        let mut global = lua.global.lock().unwrap();
        let proto_gc = global.heap.allocate(proto);
        global.heap.allocate(lua_rust::value::Closure {
            proto: proto_gc,
            upvalues: vec![],
        })
    };

    lua.stack[0] = Value::RustFunction(lua_print);
    lua.stack[1] = Value::RustFunction(lua_yield);

    println!("Running Lua VM...");
    lua.execute(closure_gc).await?;

    if lua.status == ThreadStatus::Yield {
        println!("Lua VM yielded! Resuming...");
        lua.resume().await?;
    }

    println!("Lua VM finished");

    Ok(())
}
