use crate::error::LuaError;
use crate::state::{LuaState, ThreadStatus};
use crate::value::Value;
use futures::future::{BoxFuture, FutureExt};

pub fn lua_print(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        for i in start..end {
            let val = state.stack[i];
            match val {
                Value::Nil => print!("nil"),
                Value::Boolean(b) => print!("{}", b),
                Value::Integer(i) => print!("{}", i),
                Value::Number(n) => print!("{}", n),
                Value::String(s) => print!("{}", String::from_utf8_lossy(&s)),
                Value::Table(t) => print!("table: {:p}", t.ptr.as_ptr()),
                Value::LuaFunction(f) => print!("function: {:p}", f.ptr.as_ptr()),
                Value::RustFunction(f) => print!("function: {:p}", f as *const ()),
                Value::UserData(u) => print!("userdata: {:p}", u.ptr.as_ptr()),
            }
            if i < end - 1 {
                print!("\t");
            }
        }
        println!();
        Ok(0)
    }
    .boxed()
}

pub fn lua_assert(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
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
            _ => Ok(end - start),
        }
    }
    .boxed()
}

pub fn lua_load(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "load needs at least one argument".to_string(),
            ));
        }
        let input_val = state.stack[start];
        if let Value::String(s) = input_val {
            let res = {
                let mut global = state.global.lock().unwrap();
                let parser_res = crate::parser::Parser::new(&s, &mut global.heap);
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
                    state.push(Value::LuaFunction(closure_gc));
                    Ok(1)
                }
                Err(e) => {
                    let msg_gc = {
                        let mut global = state.global.lock().unwrap();
                        global.heap.allocate(format!("{}", e).into_bytes())
                    };
                    state.push(Value::Nil);
                    state.push(Value::String(msg_gc));
                    Ok(2)
                }
            }
        } else {
            let msg_gc = {
                let mut global = state.global.lock().unwrap();
                global
                    .heap
                    .allocate("load: expected string".to_string().into_bytes())
            };
            state.push(Value::Nil);
            state.push(Value::String(msg_gc));
            Ok(2)
        }
    }
    .boxed()
}

pub fn lua_string_len(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.len needs 1 argument".to_string(),
            ));
        }
        let s = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.len: expected string".to_string(),
                ))
            }
        };
        state.push(Value::Integer(s.len() as i64));
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_sub(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.sub needs at least 1 argument".to_string(),
            ));
        }
        let s = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.sub: expected string".to_string(),
                ))
            }
        };
        let i = if start + 1 < end {
            match state.stack[start + 1] {
                Value::Integer(n) => n,
                _ => 1,
            }
        } else {
            1
        };
        let j = if start + 2 < end {
            match state.stack[start + 2] {
                Value::Integer(n) => n,
                _ => -1,
            }
        } else {
            -1
        };
        let len = s.len() as i64;
        let s_idx = if i > 0 {
            i - 1
        } else if i < 0 {
            len + i
        } else {
            0
        };
        let e_idx = if j > 0 {
            j - 1
        } else if j < 0 {
            len + j
        } else {
            -1
        };
        let start_idx = std::cmp::max(0, s_idx);
        let end_idx = std::cmp::min(len - 1, e_idx);
        if start_idx <= end_idx {
            let s_gc = {
                let mut global = state.global.lock().unwrap();
                let sub = s[start_idx as usize..=end_idx as usize].to_vec();
                global.heap.allocate(sub)
            };
            state.push(Value::String(s_gc));
        } else {
            let s_gc = {
                let mut global = state.global.lock().unwrap();
                global.heap.allocate(Vec::new())
            };
            state.push(Value::String(s_gc));
        }
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_byte(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.byte needs at least 1 argument".to_string(),
            ));
        }
        let s_gc = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.byte: expected string".to_string(),
                ))
            }
        };
        let s = &*s_gc;
        let i = if start + 1 < end {
            match state.stack[start + 1] {
                Value::Integer(n) => n,
                _ => 1,
            }
        } else {
            1
        };
        let j = if start + 2 < end {
            match state.stack[start + 2] {
                Value::Integer(n) => n,
                _ => i,
            }
        } else {
            i
        };
        let len = s.len() as i64;
        let s_idx = if i > 0 {
            i - 1
        } else if i < 0 {
            len + i
        } else {
            0
        };
        let e_idx = if j > 0 {
            j - 1
        } else if j < 0 {
            len + j
        } else {
            -1
        };
        let start_idx = std::cmp::max(0, s_idx) as usize;
        let end_idx = std::cmp::min(len - 1, e_idx) as usize;
        if start_idx <= end_idx && start_idx < s.len() {
            let nres = end_idx - start_idx + 1;
            for k in 0..nres {
                state.push(Value::Integer(s[start_idx + k] as i64));
            }
            Ok(nres)
        } else {
            Ok(0)
        }
    }
    .boxed()
}

pub fn lua_string_char(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        let mut bytes = Vec::new();
        for k in start..end {
            match state.stack[k] {
                Value::Integer(n) => {
                    if !(0..=255).contains(&n) {
                        return Err(LuaError::RuntimeError(
                            "string.char: value out of range".to_string(),
                        ));
                    }
                    bytes.push(n as u8);
                }
                _ => {
                    return Err(LuaError::RuntimeError(
                        "string.char: expected integer".to_string(),
                    ))
                }
            }
        }
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(bytes)
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_rep(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.rep needs at least 1 argument".to_string(),
            ));
        }
        let s = match state.stack[start] {
            Value::String(s) => s.to_vec(),
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.rep: expected string".to_string(),
                ))
            }
        };
        let n = if start + 1 < end {
            match state.stack[start + 1] {
                Value::Integer(n) => n,
                _ => 0,
            }
        } else {
            0
        };
        let sep = if start + 2 < end {
            match state.stack[start + 2] {
                Value::String(s) => s.to_vec(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        if n <= 0 {
            let s_gc = {
                let mut global = state.global.lock().unwrap();
                global.heap.allocate(Vec::new())
            };
            state.push(Value::String(s_gc));
        } else {
            let mut res = Vec::new();
            for i in 0..n {
                res.extend_from_slice(&s);
                if i < n - 1 {
                    res.extend_from_slice(&sep);
                }
            }
            let s_gc = {
                let mut global = state.global.lock().unwrap();
                global.heap.allocate(res)
            };
            state.push(Value::String(s_gc));
        }
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_reverse(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.reverse needs 1 argument".to_string(),
            ));
        }
        let s = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.reverse: expected string".to_string(),
                ))
            }
        };
        let mut res = s.to_vec();
        res.reverse();
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(res)
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_upper(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.upper needs 1 argument".to_string(),
            ));
        }
        let s = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.upper: expected string".to_string(),
                ))
            }
        };
        let res = s.to_ascii_uppercase();
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(res)
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_lower(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.lower needs 1 argument".to_string(),
            ));
        }
        let s = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.lower: expected string".to_string(),
                ))
            }
        };
        let res = s.to_ascii_lowercase();
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(res)
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_type(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError("type needs 1 argument".to_string()));
        }
        let type_str = match state.stack[start] {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) | Value::Integer(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::LuaFunction(_) | Value::RustFunction(_) => "function",
            Value::UserData(_) => "userdata",
        };
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(type_str.to_string().into_bytes())
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_next(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "next needs at least 1 argument".to_string(),
            ));
        }
        let t_gc = match state.stack[start] {
            Value::Table(t) => t,
            _ => return Err(LuaError::RuntimeError("next: expected table".to_string())),
        };
        let key = if start + 1 < end {
            state.stack[start + 1]
        } else {
            Value::Nil
        };
        let mut keys: Vec<Value> = t_gc.map.keys().cloned().collect();
        // Stability: sort keys by their string representation
        keys.sort_by_key(|k| format!("{:?}", k));
        if key == Value::Nil {
            if let Some(first_key) = keys.first() {
                state.push(*first_key);
                state.push(*t_gc.map.get(first_key).unwrap());
                return Ok(2);
            }
        } else if let Some(pos) = keys.iter().position(|k| k == &key) {
            if let Some(next_key) = keys.get(pos + 1) {
                state.push(*next_key);
                state.push(*t_gc.map.get(next_key).unwrap());
                return Ok(2);
            }
        }
        state.push(Value::Nil);
        Ok(1)
    }
    .boxed()
}

pub fn lua_ipairs_next(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start + 1 >= end {
            return Err(LuaError::RuntimeError(
                "ipairs_next needs 2 arguments".to_string(),
            ));
        }
        let t_gc = match state.stack[start] {
            Value::Table(t) => t,
            _ => {
                return Err(LuaError::RuntimeError(
                    "ipairs_next: expected table".to_string(),
                ))
            }
        };
        let i = match state.stack[start + 1] {
            Value::Integer(i) => i,
            _ => {
                return Err(LuaError::RuntimeError(
                    "ipairs_next: expected integer index".to_string(),
                ))
            }
        };
        let next_i = i + 1;
        let val = t_gc.map.get(&Value::Integer(next_i)).unwrap_or(&Value::Nil);
        if *val == Value::Nil {
            state.push(Value::Nil);
            Ok(1)
        } else {
            state.push(Value::Integer(next_i));
            state.push(*val);
            Ok(2)
        }
    }
    .boxed()
}

pub fn lua_ipairs(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "ipairs needs 1 argument".to_string(),
            ));
        }
        let t = state.stack[start];
        let ipairs_next = {
            let mut global = state.global.lock().unwrap();
            if let Value::Table(gt) = global.globals {
                let key =
                    Value::String(global.heap.allocate("ipairs_next".to_string().into_bytes()));
                *gt.map.get(&key).unwrap_or(&Value::Nil)
            } else {
                Value::Nil
            }
        };
        state.push(ipairs_next);
        state.push(t);
        state.push(Value::Integer(0));
        Ok(3)
    }
    .boxed()
}

pub fn lua_pairs(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError("pairs needs 1 argument".to_string()));
        }
        let t = state.stack[start];
        let next_func = {
            let mut global = state.global.lock().unwrap();
            if let Value::Table(gt) = global.globals {
                let next_key = Value::String(global.heap.allocate("next".to_string().into_bytes()));
                *gt.map.get(&next_key).unwrap_or(&Value::Nil)
            } else {
                Value::Nil
            }
        };
        state.push(next_func);
        state.push(t);
        state.push(Value::Nil);
        Ok(3)
    }
    .boxed()
}

pub fn lua_tostring(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "tostring needs 1 argument".to_string(),
            ));
        }
        let val = state.stack[start];
        let s = match val {
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => {
                state.push(Value::String(s));
                return Ok(1);
            }
            Value::Table(t) => format!("table: {:p}", t.ptr.as_ptr()),
            Value::LuaFunction(f) => format!("function: {:p}", f.ptr.as_ptr()),
            Value::RustFunction(f) => format!("function: {:p}", f as *const ()),
            Value::UserData(u) => format!("userdata: {:p}", u.ptr.as_ptr()),
        };
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(s.into_bytes())
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_math_abs(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "math.abs needs 1 argument".to_string(),
            ));
        }
        let val = state.stack[start];
        state.push(match val {
            Value::Integer(i) => Value::Integer(i.abs()),
            Value::Number(n) => Value::Number(n.abs()),
            _ => {
                return Err(LuaError::RuntimeError(
                    "math.abs: expected number".to_string(),
                ))
            }
        });
        Ok(1)
    }
    .boxed()
}

pub fn lua_math_floor(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "math.floor needs 1 argument".to_string(),
            ));
        }
        let val = state.stack[start];
        state.push(match val {
            Value::Integer(i) => Value::Integer(i),
            Value::Number(n) => Value::Integer(n.floor() as i64),
            _ => {
                return Err(LuaError::RuntimeError(
                    "math.floor: expected number".to_string(),
                ))
            }
        });
        Ok(1)
    }
    .boxed()
}

pub fn lua_table_concat(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "table.concat needs at least 1 argument".to_string(),
            ));
        }
        let t_gc = match state.stack[start] {
            Value::Table(t) => t,
            _ => {
                return Err(LuaError::RuntimeError(
                    "table.concat: expected table".to_string(),
                ))
            }
        };
        let sep = if start + 1 < end {
            match state.stack[start + 1] {
                Value::String(s) => s.to_vec(),
                Value::Nil => Vec::new(),
                _ => {
                    return Err(LuaError::RuntimeError(
                        "table.concat: expected string for separator".to_string(),
                    ))
                }
            }
        } else {
            Vec::new()
        };
        let i = if start + 2 < end {
            match state.stack[start + 2] {
                Value::Integer(n) => n,
                _ => 1,
            }
        } else {
            1
        };
        let j = if start + 3 < end {
            match state.stack[start + 3] {
                Value::Integer(n) => n,
                _ => {
                    let mut max_key = 0;
                    for k in t_gc.map.keys() {
                        if let Value::Integer(idx) = k {
                            if *idx > max_key {
                                max_key = *idx;
                            }
                        }
                    }
                    max_key
                }
            }
        } else {
            let mut max_key = 0;
            for k in t_gc.map.keys() {
                if let Value::Integer(idx) = k {
                    if *idx > max_key {
                        max_key = *idx;
                    }
                }
            }
            max_key
        };
        let mut res = Vec::new();
        for k in i..=j {
            let val = t_gc.map.get(&Value::Integer(k)).unwrap_or(&Value::Nil);
            match val {
                Value::String(s) => res.extend_from_slice(s),
                Value::Integer(n) => res.extend_from_slice(n.to_string().as_bytes()),
                Value::Number(n) => res.extend_from_slice(n.to_string().as_bytes()),
                Value::Nil => {
                    return Err(LuaError::RuntimeError(format!(
                        "table.concat: invalid value (nil) at index {}",
                        k
                    )))
                }
                _ => {
                    return Err(LuaError::RuntimeError(
                        "table.concat: invalid value in table".to_string(),
                    ))
                }
            }
            if k < j {
                res.extend_from_slice(&sep);
            }
        }
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(res)
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_format(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start >= end {
            return Err(LuaError::RuntimeError(
                "string.format needs at least 1 argument".to_string(),
            ));
        }
        let fmt_bytes = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.format: expected string".to_string(),
                ))
            }
        };
        let fmt = String::from_utf8_lossy(&fmt_bytes).into_owned();
        let mut res = Vec::new();
        let mut arg_idx = start + 1;
        let mut chars = fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                if chars.peek() == Some(&'%') {
                    res.push(b'%');
                    chars.next();
                    continue;
                }
                while let Some(&nc) = chars.peek() {
                    if "-+ #0.123456789".contains(nc) {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let spec = chars
                    .next()
                    .ok_or(LuaError::RuntimeError("invalid format string".to_string()))?;
                if arg_idx >= end {
                    return Err(LuaError::RuntimeError(
                        "not enough arguments for format string".to_string(),
                    ));
                }
                let val = state.stack[arg_idx];
                arg_idx += 1;
                match spec {
                    's' => match val {
                        Value::String(s) => res.extend_from_slice(&s),
                        _ => res.extend_from_slice(format!("{:?}", val).as_bytes()),
                    },
                    'd' | 'i' | 'u' | 'x' | 'X' => {
                        let i = match val {
                            Value::Integer(i) => i,
                            Value::Number(n) => n as i64,
                            _ => 0,
                        };
                        let s = match spec {
                            'x' => format!("{:x}", i),
                            'X' => format!("{:X}", i),
                            'u' => format!("{}", i as u64),
                            _ => format!("{}", i),
                        };
                        res.extend_from_slice(s.as_bytes());
                    }
                    'c' => {
                        let i = match val {
                            Value::Integer(i) => i,
                            Value::Number(n) => n as i64,
                            _ => 0,
                        };
                        res.push(i as u8);
                    }
                    'q' => {
                        if let Value::String(s) = val {
                            res.push(b'"');
                            for &b in s.iter() {
                                match b {
                                    b'"' => res.extend_from_slice(b"\\\""),
                                    b'\\' => res.extend_from_slice(b"\\\\"),
                                    b'\n' => res.extend_from_slice(b"\\n"),
                                    b'\r' => res.extend_from_slice(b"\\r"),
                                    b if !(32..=126).contains(&b) => {
                                        res.extend_from_slice(format!("\\{}", b).as_bytes())
                                    }
                                    b => res.push(b),
                                }
                            }
                            res.push(b'"');
                        } else {
                            res.extend_from_slice(format!("{:?}", val).as_bytes());
                        }
                    }
                    _ => res.extend_from_slice(format!("%{}", spec).as_bytes()),
                }
            } else {
                let mut buf = [0u8; 4];
                res.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        let s_gc = {
            let mut global = state.global.lock().unwrap();
            global.heap.allocate(res)
        };
        state.push(Value::String(s_gc));
        Ok(1)
    }
    .boxed()
}

pub fn lua_string_find(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        let (start, end) = get_args_range(state);
        if start + 1 >= end {
            return Err(LuaError::RuntimeError(
                "string.find needs at least 2 arguments".to_string(),
            ));
        }
        let s_gc = match state.stack[start] {
            Value::String(s) => s,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.find: expected string".to_string(),
                ))
            }
        };
        let pattern_gc = match state.stack[start + 1] {
            Value::String(p) => p,
            _ => {
                return Err(LuaError::RuntimeError(
                    "string.find: expected string for pattern".to_string(),
                ))
            }
        };
        let init = if start + 2 < end {
            match state.stack[start + 2] {
                Value::Integer(i) => i,
                _ => 1,
            }
        } else {
            1
        };
        let s = &*s_gc;
        let p = &*pattern_gc;
        let len = s.len() as i64;
        let start_pos = if init > 0 {
            init - 1
        } else if init < 0 {
            len + init
        } else {
            0
        };
        let start_pos = std::cmp::max(0, start_pos) as usize;
        if start_pos <= s.len() {
            if p.is_empty() {
                state.push(Value::Integer((start_pos + 1) as i64));
                state.push(Value::Integer(start_pos as i64));
                return Ok(2);
            }
            if let Some(pos) = s[start_pos..].windows(p.len()).position(|w| w == p) {
                state.push(Value::Integer((start_pos + pos + 1) as i64));
                state.push(Value::Integer((start_pos + pos + p.len()) as i64));
                return Ok(2);
            }
        }
        state.push(Value::Nil);
        Ok(1)
    }
    .boxed()
}

fn get_args_range(state: &LuaState) -> (usize, usize) {
    if let Some(frame) = state.frames.last() {
        let inst = frame.closure.proto.instructions[frame.pc - 1];
        let func_idx = frame.base + inst.a() as usize;
        (func_idx + 1, state.top)
    } else {
        (1, state.top)
    }
}

pub fn open_libs(state: &mut LuaState) {
    let mut global = state.global.lock().unwrap();
    if let Value::Table(t_gc) = global.globals {
        unsafe {
            let t = &mut (*t_gc.ptr.as_ptr()).data;
            let print_key = global.heap.allocate("print".to_string().into_bytes());
            t.map
                .insert(Value::String(print_key), Value::RustFunction(lua_print));
            let assert_key = global.heap.allocate("assert".to_string().into_bytes());
            t.map
                .insert(Value::String(assert_key), Value::RustFunction(lua_assert));
            let load_key = global.heap.allocate("load".to_string().into_bytes());
            t.map
                .insert(Value::String(load_key), Value::RustFunction(lua_load));
            let string_key = global.heap.allocate("string".to_string().into_bytes());
            let string_lib = global.heap.allocate(crate::value::Table::new());
            t.map
                .insert(Value::String(string_key), Value::Table(string_lib));
            let string_t = &mut (*string_lib.ptr.as_ptr()).data;
            let len_key = global.heap.allocate("len".to_string().into_bytes());
            string_t
                .map
                .insert(Value::String(len_key), Value::RustFunction(lua_string_len));
            let sub_key = global.heap.allocate("sub".to_string().into_bytes());
            string_t
                .map
                .insert(Value::String(sub_key), Value::RustFunction(lua_string_sub));
            let byte_key = global.heap.allocate("byte".to_string().into_bytes());
            string_t.map.insert(
                Value::String(byte_key),
                Value::RustFunction(lua_string_byte),
            );
            let char_key = global.heap.allocate("char".to_string().into_bytes());
            string_t.map.insert(
                Value::String(char_key),
                Value::RustFunction(lua_string_char),
            );
            let rep_key = global.heap.allocate("rep".to_string().into_bytes());
            string_t
                .map
                .insert(Value::String(rep_key), Value::RustFunction(lua_string_rep));
            let reverse_key = global.heap.allocate("reverse".to_string().into_bytes());
            string_t.map.insert(
                Value::String(reverse_key),
                Value::RustFunction(lua_string_reverse),
            );
            let upper_key = global.heap.allocate("upper".to_string().into_bytes());
            string_t.map.insert(
                Value::String(upper_key),
                Value::RustFunction(lua_string_upper),
            );
            let lower_key = global.heap.allocate("lower".to_string().into_bytes());
            string_t.map.insert(
                Value::String(lower_key),
                Value::RustFunction(lua_string_lower),
            );
            let format_key = global.heap.allocate("format".to_string().into_bytes());
            string_t.map.insert(
                Value::String(format_key),
                Value::RustFunction(lua_string_format),
            );
            let find_key = global.heap.allocate("find".to_string().into_bytes());
            string_t.map.insert(
                Value::String(find_key),
                Value::RustFunction(lua_string_find),
            );
            let type_key = global.heap.allocate("type".to_string().into_bytes());
            t.map
                .insert(Value::String(type_key), Value::RustFunction(lua_type));
            let tostring_key = global.heap.allocate("tostring".to_string().into_bytes());
            t.map.insert(
                Value::String(tostring_key),
                Value::RustFunction(lua_tostring),
            );
            let next_key = global.heap.allocate("next".to_string().into_bytes());
            t.map
                .insert(Value::String(next_key), Value::RustFunction(lua_next));
            let pairs_key = global.heap.allocate("pairs".to_string().into_bytes());
            t.map
                .insert(Value::String(pairs_key), Value::RustFunction(lua_pairs));
            let ipairs_key = global.heap.allocate("ipairs".to_string().into_bytes());
            t.map
                .insert(Value::String(ipairs_key), Value::RustFunction(lua_ipairs));
            let ipairs_next_key = global.heap.allocate("ipairs_next".to_string().into_bytes());
            t.map.insert(
                Value::String(ipairs_next_key),
                Value::RustFunction(lua_ipairs_next),
            );
            let table_key = global.heap.allocate("table".to_string().into_bytes());
            let table_lib = global.heap.allocate(crate::value::Table::new());
            t.map
                .insert(Value::String(table_key), Value::Table(table_lib));
            let table_t = &mut (*table_lib.ptr.as_ptr()).data;
            let concat_key = global.heap.allocate("concat".to_string().into_bytes());
            table_t.map.insert(
                Value::String(concat_key),
                Value::RustFunction(lua_table_concat),
            );
            let math_key = global.heap.allocate("math".to_string().into_bytes());
            let math_lib = global.heap.allocate(crate::value::Table::new());
            t.map
                .insert(Value::String(math_key), Value::Table(math_lib));
            let math_t = &mut (*math_lib.ptr.as_ptr()).data;
            let maxint_key = global.heap.allocate("maxinteger".to_string().into_bytes());
            math_t
                .map
                .insert(Value::String(maxint_key), Value::Integer(i64::MAX));
            let minint_key = global.heap.allocate("mininteger".to_string().into_bytes());
            math_t
                .map
                .insert(Value::String(minint_key), Value::Integer(i64::MIN));
            let huge_key = global.heap.allocate("huge".to_string().into_bytes());
            math_t
                .map
                .insert(Value::String(huge_key), Value::Number(f64::INFINITY));
            let abs_key = global.heap.allocate("abs".to_string().into_bytes());
            math_t
                .map
                .insert(Value::String(abs_key), Value::RustFunction(lua_math_abs));
            let floor_key = global.heap.allocate("floor".to_string().into_bytes());
            math_t.map.insert(
                Value::String(floor_key),
                Value::RustFunction(lua_math_floor),
            );
        }
    }
}

pub fn lua_yield(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        state.status = ThreadStatus::Yield;
        Ok(0)
    }
    .boxed()
}
