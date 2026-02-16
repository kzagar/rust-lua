use crate::state::{LuaState, ThreadStatus};
use crate::error::LuaError;
use crate::value::Value;
use crate::gc::{Trace, GcBoxHeader};
use std::collections::HashSet;

pub enum OpCode {
    Move = 0,
    LoadK = 1,
    Add = 2,
    Call = 3,
    Return = 4,
}

#[derive(Clone, Copy)]
pub struct Instruction(pub u32);

impl Instruction {
    pub fn opcode(&self) -> u32 {
        self.0 & 0x7F
    }
    pub fn a(&self) -> u32 {
        (self.0 >> 7) & 0xFF
    }
    pub fn b(&self) -> u32 {
        (self.0 >> 16) & 0xFF
    }
    pub fn c(&self) -> u32 {
        (self.0 >> 24) & 0xFF
    }
    pub fn bx(&self) -> u32 {
        (self.0 >> 15) & 0x1FFFF
    }
}

pub struct Proto {
    pub instructions: Vec<Instruction>,
    pub k: Vec<Value>,
}

impl Trace for Proto {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        for val in &self.k {
            val.trace(marked);
        }
    }
}

impl LuaState {
    pub async fn execute(&mut self, proto_gc: crate::gc::Gc<Proto>) -> Result<(), LuaError> {
        let proto = &*proto_gc;
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
                2 => { // ADD
                    let a = inst.a() as usize;
                    let b = inst.b() as usize;
                    let c = inst.c() as usize;
                    match (self.stack[b], self.stack[c]) {
                        (Value::Integer(vb), Value::Integer(vc)) => {
                            self.stack[a] = Value::Integer(vb + vc);
                        }
                        _ => return Err(LuaError::RuntimeError("invalid types for ADD".to_string())),
                    }
                }
                3 => { // CALL
                    let a = inst.a() as usize;
                    if let Value::RustFunction(f) = self.stack[a] {
                        let _nres = f(self).await?;
                        if self.status == ThreadStatus::Yield {
                            return Ok(());
                        }
                    } else {
                        return Err(LuaError::RuntimeError("attempt to call a non-function".to_string()));
                    }
                }
                _ => return Err(LuaError::RuntimeError(format!("unimplemented opcode {}", inst.opcode()))),
            }
        }
        Ok(())
    }
}
