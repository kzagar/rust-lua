use rua::LuaState;
use rua::parser::Parser;
use std::fs;
use std::path::Path;

async fn run_lua_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read_to_string(path)?;
    let mut lua = LuaState::new();
    rua::stdlib::open_libs(&mut lua);

    // Some tests might need extra setup or globals
    {
        let mut global = lua.global.lock().unwrap();
        // Set _VERSION
        if let rua::value::Value::Table(t) = global.globals {
             let version_key = global.heap.allocate("_VERSION".to_string());
             let version_val = global.heap.allocate("Lua 5.5".to_string());
             unsafe {
                 (*t.ptr.as_ptr()).data.map.insert(
                     rua::value::Value::String(version_key),
                     rua::value::Value::String(version_val)
                 );
             }
        }
    }

    let proto = {
        let mut global = lua.global.lock().unwrap();
        let parser = Parser::new(&input, &mut global.heap)?;
        parser.parse_chunk()?
    };

    let closure_gc = {
        let mut global = lua.global.lock().unwrap();
        let globals = global.globals;
        let proto_gc = global.heap.allocate(proto);
        let uv = global.heap.allocate(rua::value::Upvalue { val: globals });
        global.heap.allocate(rua::value::Closure {
            proto: proto_gc,
            upvalues: vec![uv],
        })
    };

    lua.execute(closure_gc).await?;

    // Handle yields if any (basic support)
    while lua.status == rua::state::ThreadStatus::Yield {
        lua.resume().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_literals() {
    let path = Path::new("../testes/literals.lua");
    if let Err(e) = run_lua_file(path).await {
        panic!("literals.lua failed: {}", e);
    }
}

#[tokio::test]
async fn test_locals() {
    let path = Path::new("../testes/locals.lua");
    if let Err(e) = run_lua_file(path).await {
        panic!("locals.lua failed: {}", e);
    }
}
