use crate::value::Value;
use crate::gc::GcHeap;
use std::sync::{Arc, Mutex};

pub struct GlobalState {
    pub heap: GcHeap,
    pub registry: Value,
    pub globals: Value,
}

pub struct LuaState {
    pub global: Arc<Mutex<GlobalState>>,
    pub stack: Vec<Value>,
    pub pc: usize,
    pub status: ThreadStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreadStatus {
    Ok,
    Yield,
    Error,
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
            pc: 0,
            status: ThreadStatus::Ok,
        }
    }

    pub fn new_thread(parent: &LuaState) -> Self {
        Self {
            global: parent.global.clone(),
            stack: vec![Value::Nil; 256],
            pc: 0,
            status: ThreadStatus::Ok,
        }
    }
}

// LuaState must be Send but not Sync
unsafe impl Send for LuaState {}
