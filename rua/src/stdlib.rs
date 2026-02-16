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
                Value::String(s) => print!("{}\t", *s),
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

pub fn open_libs(state: &mut LuaState) {
    let mut global = state.global.lock().unwrap();
    if let Value::Table(t_gc) = global.globals {
        let print_key = global.heap.allocate("print".to_string());
        unsafe {
            let t = &mut (*t_gc.ptr.as_ptr()).data;
            t.map.insert(Value::String(print_key), Value::RustFunction(lua_print));
        }
    }
}

pub fn lua_yield(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        state.status = ThreadStatus::Yield;
        Ok(0)
    }.boxed()
}
