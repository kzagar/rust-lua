use crate::state::{LuaState, ThreadStatus};
use crate::error::LuaError;
use crate::value::Value;
use futures::future::{BoxFuture, FutureExt};

pub fn lua_print(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        // In Lua 5.4, print arguments are on the stack.
        // For simplicity, we'll print stack[1] and onwards if we knew how many args.
        // But our current CALL opcode doesn't pass nres/nargs easily to the Rust function yet.
        // Let's just print what's at stack[1] for now, or all non-nil values above the function.
        let mut i = 1;
        while i < state.stack.len() {
            let val = state.stack[i];
            match val {
                Value::Nil => break,
                Value::Boolean(b) => print!("{}\t", b),
                Value::Integer(i) => print!("{}\t", i),
                Value::Number(n) => print!("{}\t", n),
                Value::String(s) => print!("{}\t", *s),
                Value::Table(_) => print!("table\t"),
                Value::LuaFunction(_) => print!("function\t"),
                Value::RustFunction(_) => print!("function\t"),
                Value::UserData(_) => print!("userdata\t"),
            }
            i += 1;
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
