use crate::state::{LuaState, ThreadStatus};
use crate::error::LuaError;
use crate::value::Value;
use crate::gc::{GCTrace, GcBoxHeader};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum OpCode {
    Move = 0,
    LoadK = 1,
    LoadI = 2,
    LoadF = 3,
    LoadFalse = 4,
    LFalseSkip = 5,
    LoadTrue = 6,
    LoadNil = 7,
    GetUpval = 8,
    SetUpval = 9,
    GetTabUp = 10,
    GetTable = 11,
    GetI = 12,
    GetField = 13,
    SetTabUp = 14,
    SetTable = 15,
    SetI = 16,
    SetField = 17,
    NewTable = 18,
    SelfOp = 19,
    AddI = 20,
    AddK = 21,
    SubK = 22,
    MulK = 23,
    ModK = 24,
    PowK = 25,
    DivK = 26,
    IDivK = 27,
    BAndK = 28,
    BOrK = 29,
    BXorK = 30,
    ShrI = 31,
    ShlI = 32,
    Add = 33,
    Sub = 34,
    Mul = 35,
    Mod = 36,
    Pow = 37,
    Div = 38,
    IDiv = 39,
    BAnd = 40,
    BOr = 41,
    BXor = 42,
    Shl = 43,
    Shr = 44,
    MmBin = 45,
    MmBinI = 46,
    MmBinK = 47,
    Unm = 48,
    BNot = 49,
    Not = 50,
    Len = 51,
    Concat = 52,
    Close = 53,
    Tbc = 54,
    Jmp = 55,
    Eq = 56,
    Lt = 57,
    Le = 58,
    EqK = 59,
    EqI = 60,
    LtI = 61,
    LeI = 62,
    GtI = 63,
    GeI = 64,
    Test = 65,
    TestSet = 66,
    Call = 67,
    TailCall = 68,
    Return = 69,
    Return0 = 70,
    Return1 = 71,
    ForLoop = 72,
    ForPrep = 73,
    TForPrep = 74,
    TForCall = 75,
    TForLoop = 76,
    SetList = 77,
    Closure = 78,
    VarArg = 79,
    VarArgPrep = 80,
    ExtraArg = 81,
}

#[derive(Clone, Copy, Debug)]
pub struct Instruction(pub u32);

impl Instruction {
    pub fn opcode(&self) -> u32 {
        self.0 & 0x7F
    }
    pub fn a(&self) -> u32 {
        (self.0 >> 7) & 0xFF
    }
    pub fn b(&self) -> u32 {
        (self.0 >> 24) & 0xFF
    }
    pub fn c(&self) -> u32 {
        (self.0 >> 15) & 0x1FF
    }
    pub fn bx(&self) -> u32 {
        (self.0 >> 15) & 0x1FFFF
    }
    pub fn sbx(&self) -> i32 {
        ((self.0 >> 15) & 0x1FFFF) as i32 - 0xFFFF
    }
    pub fn ax(&self) -> u32 {
        (self.0 >> 7) & 0x1FFFFFF
    }
    pub fn sj(&self) -> i32 {
        ((self.0 >> 7) & 0x1FFFFFF) as i32 - 0xFFFFFF
    }
    pub fn k(&self) -> bool {
        ((self.0 >> 15) & 1) != 0
    }
}

pub struct UpvalDesc {
    pub name: String,
    pub instack: bool,
    pub idx: u8,
}

pub struct Proto {
    pub instructions: Vec<Instruction>,
    pub k: Vec<Value>,
    pub upvalues: Vec<UpvalDesc>,
    pub protos: Vec<crate::gc::Gc<Proto>>,
    pub numparams: u8,
    pub is_vararg: bool,
    pub maxstacksize: u8,
}

impl GCTrace for Proto {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        for val in &self.k {
            val.trace(marked);
        }
        for proto in &self.protos {
            let header_ptr = unsafe { &proto.ptr.as_ref().header as *const GcBoxHeader };
            if !marked.contains(&header_ptr) {
                marked.insert(header_ptr);
                proto.trace(marked);
            }
        }
    }
}

impl LuaState {
    fn execute_binop(&self, opcode: u32, op1: Value, op2: Value) -> Result<Value, LuaError> {
        match opcode {
            33 => { // ADD
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                    (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 + b)),
                    (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a + b as f64)),
                    _ => Err(LuaError::RuntimeError("invalid types for ADD".to_string())),
                }
            }
            34 => { // SUB
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
                    (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 - b)),
                    (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a - b as f64)),
                    _ => Err(LuaError::RuntimeError("invalid types for SUB".to_string())),
                }
            }
            35 => { // MUL
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
                    (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 * b)),
                    (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a * b as f64)),
                    _ => Err(LuaError::RuntimeError("invalid types for MUL".to_string())),
                }
            }
            36 => { // MOD
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
                    _ => Err(LuaError::RuntimeError("invalid types for MOD".to_string())),
                }
            }
            37 => { // POW
                let a = match op1 { Value::Integer(i) => i as f64, Value::Number(n) => n, _ => return Err(LuaError::RuntimeError("invalid types for POW".to_string())) };
                let b = match op2 { Value::Integer(i) => i as f64, Value::Number(n) => n, _ => return Err(LuaError::RuntimeError("invalid types for POW".to_string())) };
                Ok(Value::Number(a.powf(b)))
            }
            38 => { // DIV
                let a = match op1 { Value::Integer(i) => i as f64, Value::Number(n) => n, _ => return Err(LuaError::RuntimeError("invalid types for DIV".to_string())) };
                let b = match op2 { Value::Integer(i) => i as f64, Value::Number(n) => n, _ => return Err(LuaError::RuntimeError("invalid types for DIV".to_string())) };
                Ok(Value::Number(a / b))
            }
            39 => { // IDIV
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a / b)),
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number((a / b).floor())),
                    _ => Err(LuaError::RuntimeError("invalid types for IDIV".to_string())),
                }
            }
            40 => { // BAND
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a & b)),
                    _ => Err(LuaError::RuntimeError("invalid types for BAND".to_string())),
                }
            }
            41 => { // BOR
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a | b)),
                    _ => Err(LuaError::RuntimeError("invalid types for BOR".to_string())),
                }
            }
            42 => { // BXOR
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a ^ b)),
                    _ => Err(LuaError::RuntimeError("invalid types for BXOR".to_string())),
                }
            }
            43 => { // SHL
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a << b)),
                    _ => Err(LuaError::RuntimeError("invalid types for SHL".to_string())),
                }
            }
            44 => { // SHR
                match (op1, op2) {
                    (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a >> b)),
                    _ => Err(LuaError::RuntimeError("invalid types for SHR".to_string())),
                }
            }
            _ => unreachable!(),
        }
    }

    fn execute_lt(&self, op1: Value, op2: Value) -> Result<bool, LuaError> {
        match (op1, op2) {
            (Value::Integer(a), Value::Integer(b)) => Ok(a < b),
            (Value::Number(a), Value::Number(b)) => Ok(a < b),
            (Value::Integer(a), Value::Number(b)) => Ok((a as f64) < b),
            (Value::Number(a), Value::Integer(b)) => Ok(a < (b as f64)),
            (Value::String(a), Value::String(b)) => Ok(**a < **b),
            _ => Err(LuaError::RuntimeError("invalid types for LT".to_string())),
        }
    }

    fn execute_le(&self, op1: Value, op2: Value) -> Result<bool, LuaError> {
        match (op1, op2) {
            (Value::Integer(a), Value::Integer(b)) => Ok(a <= b),
            (Value::Number(a), Value::Number(b)) => Ok(a <= b),
            (Value::Integer(a), Value::Number(b)) => Ok((a as f64) <= b),
            (Value::Number(a), Value::Integer(b)) => Ok(a <= (b as f64)),
            (Value::String(a), Value::String(b)) => Ok(**a <= **b),
            _ => Err(LuaError::RuntimeError("invalid types for LE".to_string())),
        }
    }

    async fn get_table_internal(&mut self, t: Value, key: Value) -> Result<Value, LuaError> {
        let mut curr_t = t;
        for _ in 0..100 { // Limit metatable chain
            let mt = match curr_t {
                Value::Table(table_gc) => {
                    if let Some(val) = table_gc.map.get(&key) {
                        if *val != Value::Nil {
                            return Ok(*val);
                        }
                    }
                    table_gc.metatable
                }
                Value::UserData(ud_gc) => ud_gc.metatable,
                _ => return Err(LuaError::RuntimeError("attempt to index a non-table value".to_string())),
            };

            if let Some(mt_table_gc) = mt {
                let index_key = {
                    let mut global = self.global.lock().unwrap();
                    Value::String(global.heap.allocate("__index".to_string()))
                };
                let h = *mt_table_gc.map.get(&index_key).unwrap_or(&Value::Nil);
                match h {
                    Value::Nil => return Ok(Value::Nil),
                    Value::Table(_) | Value::UserData(_) => {
                        curr_t = h;
                        continue;
                    }
                    Value::RustFunction(f) => {
                        // Call metamethod
                        // This is tricky because we are in an async function but not the main loop.
                        // For now, let's just support table/userdata __index.
                        // Supporting function __index requires pushing a new frame or calling f directly.
                        // Let's try calling f directly.
                        // We need to set up the stack for f.
                        let old_top = self.top;
                        self.stack.resize(old_top + 2, Value::Nil);
                        self.stack[old_top] = t;
                        self.stack[old_top + 1] = key;
                        self.top = old_top + 2;
                        let nres = f(self).await?;
                        let res = if nres > 0 { self.stack[self.top - nres] } else { Value::Nil };
                        self.top = old_top;
                        return Ok(res);
                    }
                    Value::LuaFunction(_) => {
                        return Err(LuaError::RuntimeError("Lua function metamethods not yet supported in GETTABLE".to_string()));
                    }
                    _ => return Err(LuaError::RuntimeError("invalid __index metamethod".to_string())),
                }
            } else {
                return Ok(Value::Nil);
            }
        }
        Err(LuaError::RuntimeError("metatable chain too long".to_string()))
    }

    pub fn execute(&mut self, closure_gc: crate::gc::Gc<crate::value::Closure>) -> futures::future::BoxFuture<'_, Result<(), LuaError>> {
        self.frames.push(crate::state::CallFrame {
            closure: closure_gc,
            pc: 0,
            base: 0,
            nresults: -1,
            varargs: Vec::new(),
        });
        let max_stack = closure_gc.proto.maxstacksize as usize;
        if self.stack.len() < max_stack {
            self.stack.resize(max_stack, Value::Nil);
        }
        self.top = max_stack;
        self.resume()
    }

    pub fn resume(&mut self) -> futures::future::BoxFuture<'_, Result<(), LuaError>> {
        use futures::FutureExt;
        self.status = ThreadStatus::Ok;
        async move {
            let ninitial_frames = self.frames.len();
            while self.frames.len() >= ninitial_frames {
                let (inst, base) = {
                    let frame = self.frames.last_mut().ok_or_else(|| LuaError::RuntimeError("no frame".to_string()))?;
                    let proto = &*frame.closure.proto;
                    if frame.pc >= proto.instructions.len() {
                        // Implicit return
                        let inst_ret = Instruction(70); // RETURN0
                        (inst_ret, frame.base)
                    } else {
                        let inst = proto.instructions[frame.pc];
                        let base = frame.base;
                        frame.pc += 1;
                        (inst, base)
                    }
                };

                match inst.opcode() {
                    0 => { // MOVE
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = self.stack[base + b];
                    }
                    1 => { // LOADK
                        let a = inst.a() as usize;
                        let bx = inst.bx() as usize;
                        let frame = self.frames.last().unwrap();
                        self.stack[base + a] = frame.closure.proto.k[bx];
                    }
                    2 => { // LOADI
                        let a = inst.a() as usize;
                        let sbx = inst.sbx();
                        self.stack[base + a] = Value::Integer(sbx as i64);
                    }
                    4 => { // LOADFALSE
                        let a = inst.a() as usize;
                        self.stack[base + a] = Value::Boolean(false);
                    }
                    6 => { // LOADTRUE
                        let a = inst.a() as usize;
                        self.stack[base + a] = Value::Boolean(true);
                    }
                    7 => { // LOADNIL
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        for i in a..=a + b {
                            self.stack[base + i] = Value::Nil;
                        }
                    }
                    8 => { // GETUPVAL
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.last().unwrap();
                        self.stack[base + a] = frame.closure.upvalues[b].val;
                    }
                    9 => { // SETUPVAL
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.last().unwrap();
                        unsafe {
                            let uv = &mut *frame.closure.upvalues[b].ptr.as_ptr();
                            uv.data.val = self.stack[base + a];
                        }
                    }
                    10 => { // GETTABUP
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let frame = self.frames.last().unwrap();
                        let env = frame.closure.upvalues[b].val;
                        let key = if inst.k() {
                            frame.closure.proto.k[c >> 1]
                        } else {
                            self.stack[base + c]
                        };
                        self.stack[base + a] = self.get_table_internal(env, key).await?;
                    }
                    11 => { // GETTABLE
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        let key = self.stack[base + c];
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    12 => { // GETI
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        let key = Value::Integer(c as i64);
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    13 => { // GETFIELD
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        let frame = self.frames.last().unwrap();
                        let key = frame.closure.proto.k[c];
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    19 => { // SELF
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        self.stack[base + a + 1] = t;
                        let frame = self.frames.last().unwrap();
                        let key = if inst.k() {
                            frame.closure.proto.k[c >> 1]
                        } else {
                            self.stack[base + c]
                        };
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    14 => { // SETTABUP
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let frame = self.frames.last().unwrap();
                        let env = &*frame.closure.upvalues[a];
                        if let Value::Table(t_gc) = env.val {
                            let key = if inst.k() {
                                frame.closure.proto.k[b]
                            } else {
                                self.stack[base + b]
                            };
                            let val = if ((c >> 8) & 1) != 0 {
                                frame.closure.proto.k[c & 0xFF]
                            } else {
                                self.stack[base + (c >> 1)]
                            };
                            unsafe {
                                let t = &mut (*t_gc.ptr.as_ptr()).data;
                                t.map.insert(key, val);
                            }
                        } else {
                            return Err(LuaError::RuntimeError("SETTABUP: env is not a table".to_string()));
                        }
                    }
                    33..=44 => { // Binary arithmetic/bitwise
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let op1 = self.stack[base + b];
                        let op2 = if inst.k() {
                             let frame = self.frames.last().unwrap();
                             frame.closure.proto.k[c >> 1]
                        } else {
                             self.stack[base + (c >> 1)]
                        };
                        self.stack[base + a] = self.execute_binop(inst.opcode(), op1, op2)?;
                    }
                    48 => { // UNM
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = match self.stack[base + b] {
                            Value::Integer(i) => Value::Integer(-i),
                            Value::Number(n) => Value::Number(-n),
                            _ => return Err(LuaError::RuntimeError("invalid type for UNM".to_string())),
                        };
                    }
                    49 => { // BNOT
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        if let Value::Integer(i) = self.stack[base + b] {
                            self.stack[base + a] = Value::Integer(!i);
                        } else {
                            return Err(LuaError::RuntimeError("invalid type for BNOT".to_string()));
                        }
                    }
                    50 => { // NOT
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = match self.stack[base + b] {
                            Value::Nil | Value::Boolean(false) => Value::Boolean(true),
                            _ => Value::Boolean(false),
                        };
                    }
                    51 => { // LEN
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = match self.stack[base + b] {
                            Value::String(s) => Value::Integer(s.len() as i64),
                            Value::Table(t) => Value::Integer(t.map.len() as i64),
                            _ => return Err(LuaError::RuntimeError("invalid type for LEN".to_string())),
                        };
                    }
                    52 => { // CONCAT
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let s1 = format!("{:?}", self.stack[base + a]);
                        let s2 = format!("{:?}", self.stack[base + b]);
                        let mut global = self.global.lock().unwrap();
                        let s_gc = global.heap.allocate(s1 + &s2);
                        self.stack[base + a] = Value::String(s_gc);
                    }
                    56..=58 => { // Relational
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let op1 = self.stack[base + a];
                        let op2 = self.stack[base + b];
                        let res = match inst.opcode() {
                            56 => op1 == op2,
                            57 => self.execute_lt(op1, op2)?,
                            58 => self.execute_le(op1, op2)?,
                            _ => unreachable!(),
                        };
                        let k = inst.k();
                        if res != k {
                            let frame = self.frames.last_mut().unwrap();
                            frame.pc += 1;
                        }
                    }
                    67 => { // CALL
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let func_idx = base + a;
                        let func = self.stack[func_idx];

                        let nargs = if b == 0 {
                            self.top - (func_idx + 1)
                        } else {
                            b - 1
                        };

                        match func {
                            Value::RustFunction(f) => {
                                // For Rust functions, we'll keep it simple:
                                // they see the stack from func_idx + 1 onwards.
                                // We'll temporarily adjust a "base" if we had one for Rust,
                                // but our RustFunction signature just takes &mut LuaState.
                                // Let's just call it and assume it knows where to look.
                                // Standard Lua: results are pushed onto stack starting at func_idx.
                                let old_top = self.top;
                                self.top = func_idx + 1 + nargs;
                                let nres = f(self).await?;
                                let actual_results_start = self.top - nres;
                                let expected_nres = c as i32 - 1;

                                if expected_nres == -1 {
                                    // Multiple results: move them to func_idx
                                    for i in 0..nres {
                                        self.stack[func_idx + i] = self.stack[actual_results_start + i];
                                    }
                                    self.top = func_idx + nres;
                                } else {
                                    let num_to_copy = std::cmp::min(nres, expected_nres as usize);
                                    for i in 0..num_to_copy {
                                        self.stack[func_idx + i] = self.stack[actual_results_start + i];
                                    }
                                    for i in num_to_copy..expected_nres as usize {
                                        self.stack[func_idx + i] = Value::Nil;
                                    }
                                    self.top = old_top;
                                }

                                if self.status == ThreadStatus::Yield {
                                    return Ok(());
                                }
                            }
                            Value::LuaFunction(new_closure_gc) => {
                                let new_proto = &new_closure_gc.proto;
                                let new_base = func_idx + 1;

                                let mut varargs = Vec::new();
                                if new_proto.is_vararg {
                                    let numparams = new_proto.numparams as usize;
                                    if nargs > numparams {
                                        for i in numparams..nargs {
                                            varargs.push(self.stack[new_base + i]);
                                        }
                                    }
                                }

                                self.frames.push(crate::state::CallFrame {
                                    closure: new_closure_gc,
                                    pc: 0,
                                    base: new_base,
                                    nresults: c as i32 - 1,
                                    varargs,
                                });
                                let needed_stack = new_base + new_proto.maxstacksize as usize;
                                if self.stack.len() < needed_stack {
                                    self.stack.resize(needed_stack, Value::Nil);
                                }
                                self.top = needed_stack;
                            }
                            _ => return Err(LuaError::RuntimeError("attempt to call a non-function".to_string())),
                        }
                    }
                    69 | 70 | 71 => { // RETURN
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.pop().unwrap();
                        let frame_base = frame.base;
                        let nres = if b == 0 {
                            self.top - (frame_base + a)
                        } else {
                            b - 1
                        };

                        if let Some(caller_frame) = self.frames.last() {
                            let caller_pc = caller_frame.pc - 1;
                            let call_inst = caller_frame.closure.proto.instructions[caller_pc];
                            let dest_a = caller_frame.base + call_inst.a() as usize;
                            let expected_nres = frame.nresults;

                            if expected_nres == -1 {
                                for i in 0..nres {
                                    self.stack[dest_a + i] = self.stack[frame_base + a + i];
                                }
                                self.top = dest_a + nres;
                            } else {
                                let num_to_copy = std::cmp::min(nres, expected_nres as usize);
                                for i in 0..num_to_copy {
                                    self.stack[dest_a + i] = self.stack[frame_base + a + i];
                                }
                                for i in num_to_copy..expected_nres as usize {
                                    self.stack[dest_a + i] = Value::Nil;
                                }
                            }
                        } else {
                            return Ok(());
                        }
                    }
                    78 => { // CLOSURE
                        let a = inst.a() as usize;
                        let bx = inst.bx() as usize;
                        let frame = self.frames.last().unwrap();
                        let proto = frame.closure.proto.protos[bx];
                        let mut upvalues = Vec::new();
                        for uv_desc in &proto.upvalues {
                            if uv_desc.instack {
                                let val = self.stack[base + uv_desc.idx as usize];
                                upvalues.push(self.global.lock().unwrap().heap.allocate(crate::value::Upvalue { val }));
                            } else {
                                upvalues.push(frame.closure.upvalues[uv_desc.idx as usize]);
                            }
                        }
                        let closure = self.global.lock().unwrap().heap.allocate(crate::value::Closure {
                            proto,
                            upvalues,
                        });
                        self.stack[base + a] = Value::LuaFunction(closure);
                    }
                    79 => { // VARARG
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.last().unwrap();
                        let n = if b == 0 {
                            frame.varargs.len()
                        } else {
                            b - 1
                        };
                        for i in 0..n {
                            self.stack[base + a + i] = if i < frame.varargs.len() {
                                frame.varargs[i]
                            } else {
                                Value::Nil
                            };
                        }
                        if b == 0 {
                            self.top = base + a + n;
                        }
                    }
                    80 => { // VARARGPREP
                        // In our simplified CALL, we already moved varargs to frame.varargs.
                        // We just need to ensure the stack is clean for the fixed params.
                        // Actually, CALL already puts fixed params at base, base+1, ...
                    }
                    17 => { // SETFIELD
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let frame = self.frames.last().unwrap();
                        let table = self.stack[base + a];
                        let key = frame.closure.proto.k[b];
                        let val = self.stack[base + (c >> 1)];
                        if let Value::Table(t_gc) = table {
                            unsafe {
                                let t = &mut (*t_gc.ptr.as_ptr()).data;
                                t.map.insert(key, val);
                            }
                        } else {
                            return Err(LuaError::RuntimeError("SETFIELD: target is not a table".to_string()));
                        }
                    }
                    _ => return Err(LuaError::RuntimeError(format!("unimplemented opcode {}", inst.opcode()))),
                }
            }
            Ok(())
        }.boxed()
    }
}
