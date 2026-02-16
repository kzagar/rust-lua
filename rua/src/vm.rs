use crate::error::LuaError;
use crate::gc::{GCTrace, GcBoxHeader};
use crate::state::{LuaState, ThreadStatus};
use crate::value::Value;
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

impl TryFrom<u32> for OpCode {
    type Error = ();
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value <= 81 {
            Ok(unsafe { std::mem::transmute::<u32, OpCode>(value) })
        } else {
            Err(())
        }
    }
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
    fn execute_binop(&self, opcode: OpCode, op1: Value, op2: Value) -> Result<Value, LuaError> {
        match opcode {
            OpCode::Add => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 + b)),
                (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a + b as f64)),
                _ => Err(LuaError::RuntimeError("invalid types for ADD".to_string())),
            },
            OpCode::Sub => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
                (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 - b)),
                (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a - b as f64)),
                _ => Err(LuaError::RuntimeError("invalid types for SUB".to_string())),
            },
            OpCode::Mul => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
                (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 * b)),
                (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a * b as f64)),
                _ => Err(LuaError::RuntimeError("invalid types for MUL".to_string())),
            },
            OpCode::Mod => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
                _ => Err(LuaError::RuntimeError("invalid types for MOD".to_string())),
            },
            OpCode::Pow => {
                let a = match op1 {
                    Value::Integer(i) => i as f64,
                    Value::Number(n) => n,
                    _ => return Err(LuaError::RuntimeError("invalid types for POW".to_string())),
                };
                let b = match op2 {
                    Value::Integer(i) => i as f64,
                    Value::Number(n) => n,
                    _ => return Err(LuaError::RuntimeError("invalid types for POW".to_string())),
                };
                Ok(Value::Number(a.powf(b)))
            }
            OpCode::Div => {
                let a = match op1 {
                    Value::Integer(i) => i as f64,
                    Value::Number(n) => n,
                    _ => return Err(LuaError::RuntimeError("invalid types for DIV".to_string())),
                };
                let b = match op2 {
                    Value::Integer(i) => i as f64,
                    Value::Number(n) => n,
                    _ => return Err(LuaError::RuntimeError("invalid types for DIV".to_string())),
                };
                Ok(Value::Number(a / b))
            }
            OpCode::IDiv => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a / b)),
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number((a / b).floor())),
                _ => Err(LuaError::RuntimeError("invalid types for IDIV".to_string())),
            },
            OpCode::BAnd => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a & b)),
                _ => Err(LuaError::RuntimeError("invalid types for BAND".to_string())),
            },
            OpCode::BOr => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a | b)),
                _ => Err(LuaError::RuntimeError("invalid types for BOR".to_string())),
            },
            OpCode::BXor => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a ^ b)),
                _ => Err(LuaError::RuntimeError("invalid types for BXOR".to_string())),
            },
            OpCode::Shl => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a << b)),
                _ => Err(LuaError::RuntimeError("invalid types for SHL".to_string())),
            },
            OpCode::Shr => match (op1, op2) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a >> b)),
                _ => Err(LuaError::RuntimeError("invalid types for SHR".to_string())),
            },
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
        for _ in 0..100 {
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
                _ => {
                    return Err(LuaError::RuntimeError(
                        "attempt to index a non-table value".to_string(),
                    ))
                }
            };

            if let Some(mt_table_gc) = mt {
                let index_key = {
                    let mut global = self.global.lock().unwrap();
                    Value::String(global.heap.allocate("__index".to_string().into_bytes()))
                };
                let h = *mt_table_gc.map.get(&index_key).unwrap_or(&Value::Nil);
                match h {
                    Value::Nil => return Ok(Value::Nil),
                    Value::Table(_) | Value::UserData(_) => {
                        curr_t = h;
                        continue;
                    }
                    Value::RustFunction(f) => {
                        let old_top = self.top;
                        self.stack.resize(old_top + 2, Value::Nil);
                        self.stack[old_top] = t;
                        self.stack[old_top + 1] = key;
                        self.top = old_top + 2;
                        let nres = f(self).await?;
                        let res = if nres > 0 {
                            self.stack[self.top - nres]
                        } else {
                            Value::Nil
                        };
                        self.top = old_top;
                        return Ok(res);
                    }
                    Value::LuaFunction(_) => {
                        return Err(LuaError::RuntimeError(
                            "Lua function metamethods not yet supported in GETTABLE".to_string(),
                        ));
                    }
                    _ => {
                        return Err(LuaError::RuntimeError(
                            "invalid __index metamethod".to_string(),
                        ))
                    }
                }
            } else {
                return Ok(Value::Nil);
            }
        }
        Err(LuaError::RuntimeError(
            "metatable chain too long".to_string(),
        ))
    }

    pub fn execute(
        &mut self,
        closure_gc: crate::gc::Gc<crate::value::Closure>,
    ) -> futures::future::BoxFuture<'_, Result<(), LuaError>> {
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
                    let frame = self
                        .frames
                        .last_mut()
                        .ok_or_else(|| LuaError::RuntimeError("no frame".to_string()))?;
                    let proto = &*frame.closure.proto;
                    if frame.pc >= proto.instructions.len() {
                        let inst_ret = Instruction(70); // RETURN0
                        (inst_ret, frame.base)
                    } else {
                        let inst = proto.instructions[frame.pc];
                        let base = frame.base;
                        frame.pc += 1;
                        (inst, base)
                    }
                };

                let opcode = inst.opcode();
                let op = OpCode::try_from(opcode).map_err(|_| {
                    LuaError::RuntimeError(format!("unimplemented opcode {}", opcode))
                })?;
                match op {
                    OpCode::Move => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = self.stack[base + b];
                    }
                    OpCode::LoadK => {
                        let a = inst.a() as usize;
                        let bx = inst.bx() as usize;
                        let frame = self.frames.last().unwrap();
                        self.stack[base + a] = frame.closure.proto.k[bx];
                    }
                    OpCode::LoadI => {
                        let a = inst.a() as usize;
                        let sbx = inst.sbx();
                        self.stack[base + a] = Value::Integer(sbx as i64);
                    }
                    OpCode::LoadFalse => {
                        let a = inst.a() as usize;
                        self.stack[base + a] = Value::Boolean(false);
                    }
                    OpCode::LoadTrue => {
                        let a = inst.a() as usize;
                        self.stack[base + a] = Value::Boolean(true);
                    }
                    OpCode::LoadNil => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        for i in a..=a + b {
                            self.stack[base + i] = Value::Nil;
                        }
                    }
                    OpCode::GetUpval => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.last().unwrap();
                        self.stack[base + a] = frame.closure.upvalues[b].val;
                    }
                    OpCode::SetUpval => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.last().unwrap();
                        unsafe {
                            let uv = &mut *frame.closure.upvalues[b].ptr.as_ptr();
                            uv.data.val = self.stack[base + a];
                        }
                    }
                    OpCode::GetTabUp => {
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
                    OpCode::GetTable => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        let key = self.stack[base + c];
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    OpCode::GetI => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        let key = Value::Integer(c as i64);
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    OpCode::GetField => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let t = self.stack[base + b];
                        let frame = self.frames.last().unwrap();
                        let key = frame.closure.proto.k[c];
                        self.stack[base + a] = self.get_table_internal(t, key).await?;
                    }
                    OpCode::SelfOp => {
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
                    OpCode::SetTabUp => {
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
                            return Err(LuaError::RuntimeError(
                                "SETTABUP: env is not a table".to_string(),
                            ));
                        }
                    }
                    OpCode::Add
                    | OpCode::Sub
                    | OpCode::Mul
                    | OpCode::Mod
                    | OpCode::Pow
                    | OpCode::Div
                    | OpCode::IDiv
                    | OpCode::BAnd
                    | OpCode::BOr
                    | OpCode::BXor
                    | OpCode::Shl
                    | OpCode::Shr => {
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
                        self.stack[base + a] = self.execute_binop(op, op1, op2)?;
                    }
                    OpCode::Unm => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = match self.stack[base + b] {
                            Value::Integer(i) => Value::Integer(-i),
                            Value::Number(n) => Value::Number(-n),
                            _ => {
                                return Err(LuaError::RuntimeError(
                                    "invalid type for UNM".to_string(),
                                ))
                            }
                        };
                    }
                    OpCode::BNot => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        if let Value::Integer(i) = self.stack[base + b] {
                            self.stack[base + a] = Value::Integer(!i);
                        } else {
                            return Err(LuaError::RuntimeError(
                                "invalid type for BNOT".to_string(),
                            ));
                        }
                    }
                    OpCode::Not => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = match self.stack[base + b] {
                            Value::Nil | Value::Boolean(false) => Value::Boolean(true),
                            _ => Value::Boolean(false),
                        };
                    }
                    OpCode::Len => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        self.stack[base + a] = match self.stack[base + b] {
                            Value::String(s) => Value::Integer(s.len() as i64),
                            Value::Table(t) => Value::Integer(t.map.len() as i64),
                            _ => {
                                return Err(LuaError::RuntimeError(
                                    "invalid type for LEN".to_string(),
                                ))
                            }
                        };
                    }
                    OpCode::Concat => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let s1 = match self.stack[base + a] {
                            Value::String(s) => s.to_vec(),
                            v => format!("{:?}", v).into_bytes(),
                        };
                        let s2 = match self.stack[base + b] {
                            Value::String(s) => s.to_vec(),
                            v => format!("{:?}", v).into_bytes(),
                        };
                        let mut res = s1;
                        res.extend(s2);
                        let mut global = self.global.lock().unwrap();
                        let s_gc = global.heap.allocate(res);
                        self.stack[base + a] = Value::String(s_gc);
                    }
                    OpCode::Eq | OpCode::Lt | OpCode::Le => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let op1 = self.stack[base + a];
                        let op2 = self.stack[base + b];
                        let res = match op {
                            OpCode::Eq => op1 == op2,
                            OpCode::Lt => self.execute_lt(op1, op2)?,
                            OpCode::Le => self.execute_le(op1, op2)?,
                            _ => unreachable!(),
                        };
                        let k = inst.k();
                        if res != k {
                            let frame = self.frames.last_mut().unwrap();
                            frame.pc += 1;
                        }
                    }
                    OpCode::Call => {
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
                                let old_top = self.top;
                                self.top = func_idx + 1 + nargs;
                                let nres = f(self).await?;
                                let actual_results_start = self.top - nres;
                                let expected_nres = c as i32 - 1;
                                if expected_nres == -1 {
                                    for i in 0..nres {
                                        self.stack[func_idx + i] =
                                            self.stack[actual_results_start + i];
                                    }
                                    self.top = func_idx + nres;
                                } else {
                                    let num_to_copy = std::cmp::min(nres, expected_nres as usize);
                                    for i in 0..num_to_copy {
                                        self.stack[func_idx + i] =
                                            self.stack[actual_results_start + i];
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
                            _ => {
                                return Err(LuaError::RuntimeError(
                                    "attempt to call a non-function".to_string(),
                                ))
                            }
                        }
                    }
                    OpCode::Return | OpCode::Return0 | OpCode::Return1 => {
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
                            let mut dest_a = caller_frame.base + call_inst.a() as usize;
                            if call_inst.opcode() == OpCode::TForCall as u32 {
                                dest_a += 3;
                            }
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
                    OpCode::Closure => {
                        let a = inst.a() as usize;
                        let bx = inst.bx() as usize;
                        let frame = self.frames.last().unwrap();
                        let proto = frame.closure.proto.protos[bx];
                        let mut upvalues = Vec::new();
                        for uv_desc in &proto.upvalues {
                            if uv_desc.instack {
                                let val = self.stack[base + uv_desc.idx as usize];
                                upvalues.push(
                                    self.global
                                        .lock()
                                        .unwrap()
                                        .heap
                                        .allocate(crate::value::Upvalue { val }),
                                );
                            } else {
                                upvalues.push(frame.closure.upvalues[uv_desc.idx as usize]);
                            }
                        }
                        let closure = self
                            .global
                            .lock()
                            .unwrap()
                            .heap
                            .allocate(crate::value::Closure { proto, upvalues });
                        self.stack[base + a] = Value::LuaFunction(closure);
                    }
                    OpCode::VarArg => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let frame = self.frames.last().unwrap();
                        let n = if b == 0 { frame.varargs.len() } else { b - 1 };
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
                    OpCode::VarArgPrep => {}
                    OpCode::NewTable => {
                        let a = inst.a() as usize;
                        let mut global = self.global.lock().unwrap();
                        let table = global.heap.allocate(crate::value::Table::new());
                        self.stack[base + a] = Value::Table(table);
                    }
                    OpCode::SetTable => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let table = self.stack[base + a];
                        let key = self.stack[base + b];
                        let val = if (c & 1) != 0 {
                            let frame = self.frames.last().unwrap();
                            frame.closure.proto.k[c >> 1]
                        } else {
                            self.stack[base + (c >> 1)]
                        };
                        if let Value::Table(t_gc) = table {
                            unsafe {
                                let t = &mut (*t_gc.ptr.as_ptr()).data;
                                t.map.insert(key, val);
                            }
                        } else {
                            return Err(LuaError::RuntimeError(
                                "SETTABLE: target is not a table".to_string(),
                            ));
                        }
                    }
                    OpCode::SetI => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let table = self.stack[base + a];
                        let key = Value::Integer(b as i64);
                        let val = if (c & 1) != 0 {
                            let frame = self.frames.last().unwrap();
                            frame.closure.proto.k[c >> 1]
                        } else {
                            self.stack[base + (c >> 1)]
                        };
                        if let Value::Table(t_gc) = table {
                            unsafe {
                                let t = &mut (*t_gc.ptr.as_ptr()).data;
                                t.map.insert(key, val);
                            }
                        } else {
                            return Err(LuaError::RuntimeError(
                                "SETI: target is not a table".to_string(),
                            ));
                        }
                    }
                    OpCode::SetField => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let c = inst.c() as usize;
                        let frame = self.frames.last().unwrap();
                        let table = self.stack[base + a];
                        let key = frame.closure.proto.k[b];
                        let val = if (c & 1) != 0 {
                            frame.closure.proto.k[c >> 1]
                        } else {
                            self.stack[base + (c >> 1)]
                        };
                        if let Value::Table(t_gc) = table {
                            unsafe {
                                let t = &mut (*t_gc.ptr.as_ptr()).data;
                                t.map.insert(key, val);
                            }
                        } else {
                            return Err(LuaError::RuntimeError(
                                "SETFIELD: target is not a table".to_string(),
                            ));
                        }
                    }
                    OpCode::Jmp => {
                        let sj = inst.sj();
                        let frame = self.frames.last_mut().unwrap();
                        frame.pc = (frame.pc as i32 + sj) as usize;
                    }
                    OpCode::Test => {
                        let a = inst.a() as usize;
                        let k = inst.k();
                        let val = self.stack[base + a];
                        let res = !matches!(val, Value::Nil | Value::Boolean(false));
                        if res != k {
                            let frame = self.frames.last_mut().unwrap();
                            frame.pc += 1;
                        }
                    }
                    OpCode::TestSet => {
                        let a = inst.a() as usize;
                        let b = inst.b() as usize;
                        let k = inst.k();
                        let val = self.stack[base + b];
                        let res = !matches!(val, Value::Nil | Value::Boolean(false));
                        if res == k {
                            self.stack[base + a] = val;
                        } else {
                            let frame = self.frames.last_mut().unwrap();
                            frame.pc += 1;
                        }
                    }
                    OpCode::Tbc => {
                        let a = inst.a() as usize;
                        self.tbc_stack.push(base + a);
                    }
                    OpCode::Close => {
                        let a = inst.a() as usize;
                        let level = base + a;
                        while let Some(&idx) = self.tbc_stack.last() {
                            if idx >= level {
                                let _val = self.stack[idx];
                                // TODO: Call __close metamethod
                                self.tbc_stack.pop();
                            } else {
                                break;
                            }
                        }
                    }
                    OpCode::ForPrep => {
                        let a = inst.a() as usize;
                        let sbx = inst.sbx();
                        let init = self.stack[base + a];
                        let step = self.stack[base + a + 2];

                        match (init, step) {
                            (Value::Integer(i), Value::Integer(s)) => {
                                self.stack[base + a] = Value::Integer(i - s);
                            }
                            (Value::Number(n), Value::Number(s)) => {
                                self.stack[base + a] = Value::Number(n - s);
                            }
                            _ => {
                                return Err(LuaError::RuntimeError(
                                    "numeric for: invalid types".to_string(),
                                ))
                            }
                        }

                        let frame = self.frames.last_mut().unwrap();
                        frame.pc = (frame.pc as i32 + sbx) as usize;
                    }
                    OpCode::ForLoop => {
                        let a = inst.a() as usize;
                        let sbx = inst.sbx();
                        let current = self.stack[base + a];
                        let limit = self.stack[base + a + 1];
                        let step = self.stack[base + a + 2];

                        let (next, loop_cond) = match (current, limit, step) {
                            (Value::Integer(c), Value::Integer(l), Value::Integer(s)) => {
                                let next = c + s;
                                let cond = if s > 0 { next <= l } else { next >= l };
                                (Value::Integer(next), cond)
                            }
                            (Value::Number(c), Value::Number(l), Value::Number(s)) => {
                                let next = c + s;
                                let cond = if s > 0.0 { next <= l } else { next >= l };
                                (Value::Number(next), cond)
                            }
                            _ => {
                                return Err(LuaError::RuntimeError(
                                    "numeric for: invalid types".to_string(),
                                ))
                            }
                        };

                        if loop_cond {
                            self.stack[base + a] = next;
                            self.stack[base + a + 3] = next; // set user variable
                            let frame = self.frames.last_mut().unwrap();
                            frame.pc = (frame.pc as i32 + sbx) as usize;
                        }
                    }
                    OpCode::TForPrep => {
                        let sj = inst.sj();
                        let frame = self.frames.last_mut().unwrap();
                        frame.pc = (frame.pc as i32 + sj) as usize;
                    }
                    OpCode::TForCall => {
                        let a = inst.a() as usize;
                        let c = inst.c() as usize;
                        let func_idx = base + a;
                        let func = self.stack[func_idx];

                        match func {
                            Value::RustFunction(f) => {
                                let old_top = self.top;
                                self.top = func_idx + 3; // f, s, var are at func_idx, func_idx+1, func_idx+2
                                let nres = f(self).await?;
                                let actual_results_start = self.top - nres;
                                let expected_nres = c;
                                for i in 0..expected_nres {
                                    self.stack[func_idx + 3 + i] = if i < nres {
                                        self.stack[actual_results_start + i]
                                    } else {
                                        Value::Nil
                                    };
                                }
                                self.top = old_top;
                            }
                            Value::LuaFunction(closure_gc) => {
                                let new_proto = &closure_gc.proto;
                                let new_base = func_idx + 3;
                                self.frames.push(crate::state::CallFrame {
                                    closure: closure_gc,
                                    pc: 0,
                                    base: new_base,
                                    nresults: c as i32,
                                    varargs: Vec::new(),
                                });
                                let needed_stack = new_base + new_proto.maxstacksize as usize;
                                if self.stack.len() < needed_stack {
                                    self.stack.resize(needed_stack, Value::Nil);
                                }
                                self.top = needed_stack;
                            }
                            _ => {
                                return Err(LuaError::RuntimeError(
                                    "attempt to call a non-function in TFORCALL".to_string(),
                                ))
                            }
                        }
                    }
                    OpCode::TForLoop => {
                        let a = inst.a() as usize;
                        let sbx = inst.sbx();
                        if self.stack[base + a + 3] != Value::Nil {
                            self.stack[base + a + 2] = self.stack[base + a + 3];
                            let frame = self.frames.last_mut().unwrap();
                            frame.pc = (frame.pc as i32 + sbx) as usize;
                        }
                    }
                    _ => {
                        return Err(LuaError::RuntimeError(format!(
                            "unimplemented opcode {:?}",
                            op
                        )))
                    }
                }
            }
            Ok(())
        }
        .boxed()
    }
}
