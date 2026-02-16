use crate::gc::{Gc, GcHeap};
use crate::value::{Closure, Value};
use std::sync::{Arc, Mutex};

pub struct GlobalState {
    pub heap: GcHeap,
    pub registry: Value,
    pub globals: Value,
}

pub struct CallFrame {
    pub closure: Gc<Closure>,
    pub pc: usize,
    pub base: usize,
    pub nresults: i32,
    pub varargs: Vec<Value>,
}

pub struct LuaState {
    pub global: Arc<Mutex<GlobalState>>,
    pub stack: Vec<Value>,
    pub top: usize,
    pub frames: Vec<CallFrame>,
    pub status: ThreadStatus,
    pub tbc_stack: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreadStatus {
    Ok,
    Yield,
    Error,
}

impl Default for LuaState {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaState {
    pub fn new() -> Self {
        let mut heap = GcHeap::new();
        let globals = Value::Table(heap.allocate(crate::value::Table::new()));
        Self {
            global: Arc::new(Mutex::new(GlobalState {
                heap,
                registry: Value::Nil,
                globals,
            })),
            stack: vec![Value::Nil; 256],
            top: 0,
            frames: Vec::new(),
            status: ThreadStatus::Ok,
            tbc_stack: Vec::new(),
        }
    }

    pub fn new_thread(parent: &LuaState) -> Self {
        Self {
            global: parent.global.clone(),
            stack: vec![Value::Nil; 256],
            top: 0,
            frames: Vec::new(),
            status: ThreadStatus::Ok,
            tbc_stack: Vec::new(),
        }
    }

    pub fn push(&mut self, val: Value) {
        if self.top >= self.stack.len() {
            self.stack.resize(self.stack.len() * 2, Value::Nil);
        }
        self.stack[self.top] = val;
        self.top += 1;
    }

    pub fn pop(&mut self) -> Value {
        if self.top == 0 {
            return Value::Nil;
        }
        self.top -= 1;
        self.stack[self.top]
    }
}

// LuaState must be Send but not Sync
unsafe impl Send for LuaState {}
