use rua::parser::Parser;
use rua::LuaState;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

async fn run_lua_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read(path)?;
    let mut lua = LuaState::new();
    rua::stdlib::open_libs(&mut lua);

    // Some tests might need extra setup or globals
    {
        let mut global = lua.global.lock().unwrap();
        // Set _VERSION
        if let rua::value::Value::Table(t) = global.globals {
            let version_key = global.heap.allocate("_VERSION".to_string().into_bytes());
            let version_val = global.heap.allocate("Lua 5.5".to_string().into_bytes());
            unsafe {
                (*t.ptr.as_ptr()).data.map.insert(
                    rua::value::Value::String(version_key),
                    rua::value::Value::String(version_val),
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

struct TestMetadata {
    expected_fail: bool,
    reason: String,
}

fn get_expected_failures() -> HashMap<String, TestMetadata> {
    let mut m = HashMap::new();

    let not_implemented = "feature not yet implemented";

    // Explicitly listed tests with status
    m.insert(
        "all.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "api.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: "C API not supported".to_string(),
        },
    );
    m.insert(
        "attrib.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "big.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "bitwise.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "bwcoercion.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "calls.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "closure.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "code.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "constructs.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "coroutine.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "cstack.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "db.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "errors.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "events.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "files.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "gc.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "gengc.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "goto.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "heavy.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "literals.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "locals.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "main.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "math.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "memerr.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "nextvar.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "pm.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "sort.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "strings.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "tpack.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "tracegc.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "utf8.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "vararg.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );
    m.insert(
        "verybig.lua".to_string(),
        TestMetadata {
            expected_fail: true,
            reason: not_implemented.to_string(),
        },
    );

    m
}

#[tokio::test]
async fn test_lua_suite() {
    let test_dir = Path::new("../testes");
    let mut paths: Vec<PathBuf> = fs::read_dir(test_dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "lua") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    paths.sort();

    let expected_failures = get_expected_failures();
    let mut passed = 0;
    let mut failed = 0;
    let mut xfailed = 0;
    let mut xpassed = 0;

    for path in paths {
        let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
        let metadata = expected_failures.get(&file_name);

        println!("Running {}...", file_name);
        let result = run_lua_file(&path).await;

        match (result, metadata) {
            (Ok(_), None) => {
                println!("  OK");
                passed += 1;
            }
            (Ok(_), Some(meta)) if !meta.expected_fail => {
                println!("  OK");
                passed += 1;
            }
            (Ok(_), Some(_)) => {
                println!("  XPASSED (unexpectedly passed)");
                xpassed += 1;
            }
            (Err(e), Some(meta)) if meta.expected_fail => {
                println!("  XFAIL: {} (Reason: {})", e, meta.reason);
                xfailed += 1;
            }
            (Err(e), _) => {
                println!("  FAILED: {}", e);
                failed += 1;
            }
        }
    }

    println!("\nSuite Summary:");
    println!("  Passed:      {}", passed);
    println!("  Failed:      {}", failed);
    println!("  XFailed:     {} (expected failures)", xfailed);
    println!("  XPassed:     {} (unexpected passes)", xpassed);

    if failed > 0 || xpassed > 0 {
        panic!("Some tests failed or unexpectedly passed.");
    }
}
