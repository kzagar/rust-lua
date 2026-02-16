use crate::gc::{Gc, GCTrace, GcBoxHeader};
use std::collections::{HashSet, HashMap};
use crate::vm::Proto;
use futures::future::BoxFuture;
use crate::error::LuaError;
use crate::state::LuaState;
use std::hash::{Hash, Hasher};
use std::any::Any;

pub type AsyncCallback = for<'a> fn(&'a mut LuaState) -> BoxFuture<'a, Result<usize, LuaError>>;

pub trait LuaUserData: GCTrace + Any + Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub struct UserData {
    pub data: Box<dyn LuaUserData>,
    pub metatable: Option<Gc<Table>>,
}

impl GCTrace for UserData {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        self.data.trace(marked);
        if let Some(mt) = self.metatable {
            let header_ptr = unsafe { &mt.ptr.as_ref().header as *const GcBoxHeader };
            if !marked.contains(&header_ptr) {
                marked.insert(header_ptr);
                mt.trace(marked);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(Gc<String>),
    Table(Gc<Table>),
    LuaFunction(Gc<Closure>),
    RustFunction(AsyncCallback),
    UserData(Gc<UserData>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => **a == **b,
            (Value::Table(a), Value::Table(b)) => a.ptr == b.ptr,
            (Value::LuaFunction(a), Value::LuaFunction(b)) => a.ptr == b.ptr,
            (Value::RustFunction(a), Value::RustFunction(b)) => *a as usize == *b as usize,
            (Value::UserData(a), Value::UserData(b)) => a.ptr == b.ptr,
            (Value::Integer(a), Value::Number(b)) => *a as f64 == *b,
            (Value::Number(a), Value::Integer(b)) => *a == *b as f64,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Nil => 0.hash(state),
            Value::Boolean(b) => b.hash(state),
            Value::Integer(i) => i.hash(state),
            Value::Number(n) => {
                if n.is_nan() {
                    0.hash(state); // Should not happen as keys
                } else {
                    n.to_bits().hash(state);
                }
            }
            Value::String(s) => (**s).hash(state),
            Value::Table(t) => t.ptr.as_ptr().hash(state),
            Value::LuaFunction(f) => f.ptr.as_ptr().hash(state),
            Value::RustFunction(f) => (*f as usize).hash(state),
            Value::UserData(u) => u.ptr.as_ptr().hash(state),
        }
    }
}

impl GCTrace for Value {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        match self {
            Value::String(s) => {
                let header_ptr = unsafe { &s.ptr.as_ref().header as *const GcBoxHeader };
                if !marked.contains(&header_ptr) {
                    marked.insert(header_ptr);
                    s.trace(marked);
                }
            }
            Value::Table(t) => {
                let header_ptr = unsafe { &t.ptr.as_ref().header as *const GcBoxHeader };
                if !marked.contains(&header_ptr) {
                    marked.insert(header_ptr);
                    t.trace(marked);
                }
            }
            Value::LuaFunction(f) => {
                 let header_ptr = unsafe { &f.ptr.as_ref().header as *const GcBoxHeader };
                if !marked.contains(&header_ptr) {
                    marked.insert(header_ptr);
                    f.trace(marked);
                }
            }
            Value::UserData(u) => {
                let header_ptr = unsafe { &u.ptr.as_ref().header as *const GcBoxHeader };
                if !marked.contains(&header_ptr) {
                    marked.insert(header_ptr);
                    u.trace(marked);
                }
            }
            _ => {}
        }
    }
}

pub struct Table {
    pub map: HashMap<Value, Value>,
    pub metatable: Option<Gc<Table>>,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    pub fn new() -> Self {
        Self { map: HashMap::new(), metatable: None }
    }
}

impl GCTrace for Table {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        for (k, v) in &self.map {
            k.trace(marked);
            v.trace(marked);
        }
        if let Some(mt) = self.metatable {
            let header_ptr = unsafe { &mt.ptr.as_ref().header as *const GcBoxHeader };
            if !marked.contains(&header_ptr) {
                marked.insert(header_ptr);
                mt.trace(marked);
            }
        }
    }
}

pub struct Closure {
    pub proto: Gc<Proto>,
    pub upvalues: Vec<Gc<Upvalue>>,
}

impl GCTrace for Closure {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        let proto_header = unsafe { &self.proto.ptr.as_ref().header as *const GcBoxHeader };
        if !marked.contains(&proto_header) {
            marked.insert(proto_header);
            self.proto.trace(marked);
        }
        for uv in &self.upvalues {
            let uv_header = unsafe { &uv.ptr.as_ref().header as *const GcBoxHeader };
            if !marked.contains(&uv_header) {
                marked.insert(uv_header);
                uv.trace(marked);
            }
        }
    }
}

pub struct Upvalue {
    pub val: Value,
}

impl GCTrace for Upvalue {
    fn trace(&self, marked: &mut HashSet<*const GcBoxHeader>) {
        self.val.trace(marked);
    }
}
