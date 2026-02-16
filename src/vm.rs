use crate::state::{LuaState, ThreadStatus};
use crate::error::LuaError;
use crate::value::Value;
use crate::gc::{Trace, GcBoxHeader};
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
}

impl Trace for Proto {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        for val in &self.k {
            val.trace(marked);
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

    pub fn execute(&mut self, closure_gc: crate::gc::Gc<crate::value::Closure>) -> futures::future::BoxFuture<'_, Result<(), LuaError>> {
        use futures::FutureExt;
        async move {
            let closure = &*closure_gc;
            let proto = &*closure.proto;
            self.status = ThreadStatus::Ok;

            while self.pc < proto.instructions.len() {
                let inst = proto.instructions[self.pc];
                self.pc += 1;
                match inst.opcode() {
                0 => { // MOVE
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    self.stack[a] = self.stack[b];
                }
                1 => { // LOADK
                    let a = inst.a() as usize;
                    let bx = inst.bx() as usize;
                    self.stack[a] = proto.k[bx];
                }
                2 => { // LOADI
                    let a = inst.a() as usize;
                    let sbx = inst.sbx();
                    self.stack[a] = Value::Integer(sbx as i64);
                }
                4 => { // LOADFALSE
                    let a = inst.a() as usize;
                    self.stack[a] = Value::Boolean(false);
                }
                6 => { // LOADTRUE
                    let a = inst.a() as usize;
                    self.stack[a] = Value::Boolean(true);
                }
                7 => { // LOADNIL
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    for i in a..=a + b {
                        self.stack[i] = Value::Nil;
                    }
                }
                10 => { // GETTABUP
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    let c = inst.c() as usize;
                    let env = &*closure.upvalues[b];
                    if let Value::Table(t) = env.val {
                        let key = if inst.k() {
                            proto.k[c >> 1]
                        } else {
                            self.stack[c]
                        };
                        self.stack[a] = *t.map.get(&key).unwrap_or(&Value::Nil);
                    } else {
                        return Err(LuaError::RuntimeError("GETTABUP: env is not a table".to_string()));
                    }
                }
                14 => { // SETTABUP
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    let c = inst.c() as usize;
                    // Note: This is mutable access to a table in the heap.
                    // LuaState might need more robust heap access.
                    let env = &*closure.upvalues[a];
                    if let Value::Table(t_gc) = env.val {
                        let key = if inst.k() {
                            proto.k[b]
                        } else {
                            self.stack[b]
                        };
                        let val = self.stack[c >> 1];
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
                    let op1 = self.stack[b];
                    let op2 = self.stack[c];
                    self.stack[a] = self.execute_binop(inst.opcode(), op1, op2)?;
                }
                48 => { // UNM
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    self.stack[a] = match self.stack[b] {
                        Value::Integer(i) => Value::Integer(-i),
                        Value::Number(n) => Value::Number(-n),
                        _ => return Err(LuaError::RuntimeError("invalid type for UNM".to_string())),
                    };
                }
                49 => { // BNOT
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    if let Value::Integer(i) = self.stack[b] {
                        self.stack[a] = Value::Integer(!i);
                    } else {
                        return Err(LuaError::RuntimeError("invalid type for BNOT".to_string()));
                    }
                }
                50 => { // NOT
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    self.stack[a] = match self.stack[b] {
                        Value::Nil | Value::Boolean(false) => Value::Boolean(true),
                        _ => Value::Boolean(false),
                    };
                }
                51 => { // LEN
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    self.stack[a] = match self.stack[b] {
                        Value::String(s) => Value::Integer(s.len() as i64),
                        Value::Table(t) => Value::Integer(t.map.len() as i64),
                        _ => return Err(LuaError::RuntimeError("invalid type for LEN".to_string())),
                    };
                }
                52 => { // CONCAT
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    // Simplified: just concat 2 values
                    let s1 = format!("{:?}", self.stack[a]); // Very simplified coercion
                    let s2 = format!("{:?}", self.stack[b]);
                    let mut global = self.global.lock().unwrap();
                    let s_gc = global.heap.allocate(s1 + &s2);
                    self.stack[a] = Value::String(s_gc);
                }
                56..=58 => { // Relational
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    let _c = inst.c() as usize;
                    let op1 = self.stack[a];
                    let op2 = self.stack[b];
                    let res = match inst.opcode() {
                        56 => op1 == op2,
                        57 => self.execute_lt(op1, op2)?,
                        58 => self.execute_le(op1, op2)?,
                        _ => unreachable!(),
                    };
                    let k = inst.k();
                    if res != k {
                        self.pc += 1;
                    }
                }
                    67 => { // CALL
                        let a = inst.a() as usize;
                        let _b = inst.b() as usize;
                        // c = inst.c() as usize;
                        match self.stack[a] {
                            Value::RustFunction(f) => {
                                // Setup arguments if needed
                                let _nres = f(self).await?;
                                if self.status == ThreadStatus::Yield {
                                    return Ok(());
                                }
                            }
                            Value::LuaFunction(new_closure_gc) => {
                                 let old_pc = self.pc;
                                 self.pc = 0;
                                 self.execute(new_closure_gc).await?;
                                 self.pc = old_pc;
                                 // This is recursive and not quite right for Lua's stack,
                                 // but for this task it might be okay.
                            }
                            _ => return Err(LuaError::RuntimeError("attempt to call a non-function".to_string())),
                        }
                    }
                    69 | 70 | 71 => { // RETURN
                        return Ok(());
                    }
                    _ => return Err(LuaError::RuntimeError(format!("unimplemented opcode {}", inst.opcode()))),
                }
            }
            Ok(())
        }.boxed()
    }
}
