use rua::LuaState;
use rua::parser::Parser;
use clap::Parser as ClapParser;
use std::path::PathBuf;
use std::fs;
use rua::state::ThreadStatus;

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Lua file to execute
    file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Read Lua file
    let input = fs::read_to_string(&args.file)?;

    let mut lua = LuaState::new();
    
    // Open standard libraries (print, etc.)
    rua::stdlib::open_libs(&mut lua);

    // Parse the Lua script
    let proto = {
        let mut global = lua.global.lock().unwrap();
        let parser = Parser::new(&input, &mut global.heap)?;
        parser.parse_chunk()?
    };

    // Prepare the main closure with _ENV as the first upvalue
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

    println!("Executing {}...", args.file.display());
    if let Err(e) = lua.execute(closure_gc).await {
        eprintln!("Lua Execution Error: {}", e);
        std::process::exit(1);
    }

    // Handle yields if any
    while lua.status == ThreadStatus::Yield {
        if let Err(e) = lua.resume().await {
            eprintln!("Lua Execution Error (after yield): {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
