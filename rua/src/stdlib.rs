use crate::state::{LuaState, ThreadStatus};
use crate::error::LuaError;
use crate::value::Value;
use futures::future::{BoxFuture, FutureExt};

pub fn lua_print(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        // Find the range of arguments on the stack.
        // In our VM, a Rust function is called from a Lua frame.
        // The CALL instruction's 'a' register points to the function,
        // and arguments follow immediately after.
        let (start, end) = if let Some(frame) = state.frames.last() {
            let inst = frame.closure.proto.instructions[frame.pc - 1];
            let func_idx = frame.base + inst.a() as usize;
            (func_idx + 1, state.top)
        } else {
            // Fallback for top-level calls (though execute() pushes a frame)
            (1, state.top)
        };

        for i in start..end {
            let val = state.stack[i];
            match val {
                Value::Nil => print!("nil\t"),
                Value::Boolean(b) => print!("{}\t", b),
                Value::Integer(i) => print!("{}\t", i),
                Value::Number(n) => print!("{}\t", n),
                Value::String(s) => print!("{}\t", String::from_utf8_lossy(&s)),
                Value::Table(_) => print!("table\t"),
                Value::LuaFunction(_) => print!("function\t"),
                Value::RustFunction(_) => print!("function\t"),
                Value::UserData(_) => print!("userdata\t"),
            }
            if i < end - 1 {
                print!("\t");
            }
        }
        println!();
        Ok(0)
    }.boxed()
}

pub fn lua_assert(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = if let Some(frame) = state.frames.last() {
            let inst = frame.closure.proto.instructions[frame.pc - 1];
            let func_idx = frame.base + inst.a() as usize;
            (func_idx + 1, state.top)
        } else {
            (1, state.top)
        };

        if start >= end {
            return Err(LuaError::RuntimeError("assertion failed!".to_string()));
        }

        let val = state.stack[start];
        match val {
            Value::Nil | Value::Boolean(false) => {
                let msg = if start + 1 < end {
                    format!("{:?}", state.stack[start + 1])
                } else {
                    "assertion failed!".to_string()
                };
                Err(LuaError::RuntimeError(msg))
            }
            _ => {
                // Return all arguments
                let nres = end - start;
                // They are already in the right place on the stack for the caller to pick up
                // if we just return nres.
                // Wait, standard assert returns its arguments.
                Ok(nres)
            }
        }
    }.boxed()
}

pub fn lua_load(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = if let Some(frame) = state.frames.last() {
            let inst = frame.closure.proto.instructions[frame.pc - 1];
            let func_idx = frame.base + inst.a() as usize;
            (func_idx + 1, state.top)
        } else {
            (1, state.top)
        };

        if start >= end {
             return Err(LuaError::RuntimeError("load needs at least one argument".to_string()));
        }

        let input_val = state.stack[start];
        if let Value::String(s) = input_val {
            let res = {
                let mut global = state.global.lock().unwrap();
                let s_str = String::from_utf8_lossy(&s);
                let parser_res = crate::parser::Parser::new(&s_str, &mut global.heap);
                match parser_res {
                    Ok(parser) => parser.parse_chunk(),
                    Err(e) => Err(e),
                }
            };

            match res {
                Ok(proto) => {
                    let closure_gc = {
                        let mut global = state.global.lock().unwrap();
                        let globals = global.globals;
                        let proto_gc = global.heap.allocate(proto);
                        let uv = global.heap.allocate(crate::value::Upvalue { val: globals });
                        global.heap.allocate(crate::value::Closure {
                            proto: proto_gc,
                            upvalues: vec![uv],
                        })
                    };
                    state.stack[start - 1] = Value::LuaFunction(closure_gc);
                    Ok(1)
                }
                Err(e) => {
                    state.stack[start - 1] = Value::Nil;
                    let mut global = state.global.lock().unwrap();
                    let msg_gc = global.heap.allocate(format!("{}", e).into_bytes());
                    state.stack[start] = Value::String(msg_gc);
                    Ok(2)
                }
            }
        } else {
            state.stack[start - 1] = Value::Nil;
            let mut global = state.global.lock().unwrap();
            let msg_gc = global.heap.allocate("load: expected string".to_string().into_bytes());
            state.stack[start] = Value::String(msg_gc);
            Ok(2)
        }
    }.boxed()
}

pub fn open_libs(state: &mut LuaState) {
    let mut global = state.global.lock().unwrap();
    if let Value::Table(t_gc) = global.globals {
        unsafe {
            let t = &mut (*t_gc.ptr.as_ptr()).data;

            let print_key = global.heap.allocate("print".to_string().into_bytes());
            t.map.insert(Value::String(print_key), Value::RustFunction(lua_print));

            let assert_key = global.heap.allocate("assert".to_string().into_bytes());
            t.map.insert(Value::String(assert_key), Value::RustFunction(lua_assert));

            let load_key = global.heap.allocate("load".to_string().into_bytes());
            t.map.insert(Value::String(load_key), Value::RustFunction(lua_load));
        }
    }
}

pub fn lua_yield(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        state.status = ThreadStatus::Yield;
        Ok(0)
    }.boxed()
}
