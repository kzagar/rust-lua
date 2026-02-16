use crate::gc::{Gc, Trace, GcBoxHeader};
use std::collections::HashSet;
use crate::vm::Proto;
use futures::future::BoxFuture;
use crate::error::LuaError;
use crate::state::LuaState;

pub type AsyncCallback = for<'a> fn(&'a mut LuaState) -> BoxFuture<'a, Result<usize, LuaError>>;

#[derive(Clone, Copy)]
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(Gc<String>),
    Table(Gc<Table>),
    LuaFunction(Gc<Proto>),
    RustFunction(AsyncCallback),
}

impl Trace for Value {
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
            Value::LuaFunction(p) => {
                 let header_ptr = unsafe { &p.ptr.as_ref().header as *const GcBoxHeader };
                if !marked.contains(&header_ptr) {
                    marked.insert(header_ptr);
                    p.trace(marked);
                }
            }
            _ => {}
        }
    }
}

pub struct Table {
    // Basic table implementation
}

impl Trace for Table {
    fn trace(&self, _marked: &mut HashSet<*const GcBoxHeader>) {
        // Trace table elements
    }
}
