pub mod value;
pub mod gc;
pub mod state;
pub mod vm;
pub mod parser;
pub mod error;
pub mod stdlib;

pub use state::LuaState;
pub use error::LuaError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use crate::vm::{Proto, Instruction};

    #[tokio::test]
    async fn test_basic_vm() {
        let mut lua = LuaState::new();
        let proto = Proto {
            instructions: vec![
                Instruction(1 | (0 << 7) | (0 << 15)), // LOADK R[0] K[0]
                Instruction(1 | (1 << 7) | (1 << 15)), // LOADK R[1] K[1]
                Instruction(33 | (2 << 7) | (0 << 15) | (1 << 24)), // ADD R[2] R[0] R[1]
            ],
            k: vec![Value::Integer(10), Value::Integer(20)],
            upvalues: vec![],
            protos: vec![],
            numparams: 0,
            is_vararg: false,
            maxstacksize: 3,
        };
        let closure_gc = {
            let mut global = lua.global.lock().unwrap();
            let proto_gc = global.heap.allocate(proto);
            global.heap.allocate(crate::value::Closure {
                proto: proto_gc,
                upvalues: vec![],
            })
        };
        lua.execute(closure_gc).await.unwrap();
        if let Value::Integer(res) = lua.stack[2] {
            assert_eq!(res, 30);
        } else {
            panic!("Result is not an integer");
        }
    }

    #[tokio::test]
    async fn test_parser_and_variables() {
        let mut lua = LuaState::new();
        crate::stdlib::open_libs(&mut lua);

        let input = "
            local x = 10
            local y = 20
            z = x + y
        ";

        let proto = {
            let mut global = lua.global.lock().unwrap();
            let parser = crate::parser::Parser::new(input, &mut global.heap).unwrap();
            parser.parse_chunk().unwrap()
        };

        let closure_gc = {
            let mut global = lua.global.lock().unwrap();
            let globals = global.globals;
            let proto_gc = global.heap.allocate(proto);
            let uv = global.heap.allocate(crate::value::Upvalue { val: globals });
            global.heap.allocate(crate::value::Closure {
                proto: proto_gc,
                upvalues: vec![uv],
            })
        };

        lua.execute(closure_gc).await.unwrap();

        let global_val = {
            let mut global = lua.global.lock().unwrap();
            if let Value::Table(t) = global.globals {
                let z_key = Value::String(global.heap.allocate("z".to_string()));
                *t.map.get(&z_key).unwrap_or(&Value::Nil)
            } else {
                panic!("Globals is not a table");
            }
        };

        if let Value::Integer(res) = global_val {
            assert_eq!(res, 30);
        } else {
            panic!("Result z is not an integer: {:?}", global_val);
        }
    }

    #[tokio::test]
    async fn test_function_definition() {
        let mut lua = LuaState::new();
        crate::stdlib::open_libs(&mut lua);

        let input = "
            local function double(x)
                return x * 2
            end
            res = double(21)
        ";

        let proto = {
            let mut global = lua.global.lock().unwrap();
            let parser = crate::parser::Parser::new(input, &mut global.heap).unwrap();
            parser.parse_chunk().unwrap()
        };

        let closure_gc = {
            let mut global = lua.global.lock().unwrap();
            let globals = global.globals;
            let proto_gc = global.heap.allocate(proto);
            let uv = global.heap.allocate(crate::value::Upvalue { val: globals });
            global.heap.allocate(crate::value::Closure {
                proto: proto_gc,
                upvalues: vec![uv],
            })
        };

        lua.execute(closure_gc).await.unwrap();

        let global_val = {
            let mut global = lua.global.lock().unwrap();
            if let Value::Table(t) = global.globals {
                let res_key = Value::String(global.heap.allocate("res".to_string()));
                *t.map.get(&res_key).unwrap_or(&Value::Nil)
            } else {
                panic!("Globals is not a table");
            }
        };

        if let Value::Integer(res) = global_val {
            assert_eq!(res, 42);
        } else {
            panic!("Result res is not an integer: {:?}", global_val);
        }
    }

    #[tokio::test]
    async fn test_varargs() {
        let mut lua = LuaState::new();
        crate::stdlib::open_libs(&mut lua);

        let input = "
            local function first(...)
                local a = ...
                return a
            end
            res = first(99, 100)
        ";

        let proto = {
            let mut global = lua.global.lock().unwrap();
            let parser = crate::parser::Parser::new(input, &mut global.heap).unwrap();
            parser.parse_chunk().unwrap()
        };

        let closure_gc = {
            let mut global = lua.global.lock().unwrap();
            let globals = global.globals;
            let proto_gc = global.heap.allocate(proto);
            let uv = global.heap.allocate(crate::value::Upvalue { val: globals });
            global.heap.allocate(crate::value::Closure {
                proto: proto_gc,
                upvalues: vec![uv],
            })
        };

        lua.execute(closure_gc).await.unwrap();

        let global_val = {
            let mut global = lua.global.lock().unwrap();
            if let Value::Table(t) = global.globals {
                let res_key = Value::String(global.heap.allocate("res".to_string()));
                *t.map.get(&res_key).unwrap_or(&Value::Nil)
            } else {
                panic!("Globals is not a table");
            }
        };

        assert_eq!(global_val, Value::Integer(99));
    }

    #[tokio::test]
    async fn test_nested_upvalues() {
        let mut lua = LuaState::new();
        crate::stdlib::open_libs(&mut lua);

        let input = "
            local x = 10
            local function outer()
                local function inner()
                    return x
                end
                return inner()
            end
            res = outer()
        ";

        let proto = {
            let mut global = lua.global.lock().unwrap();
            let parser = crate::parser::Parser::new(input, &mut global.heap).unwrap();
            parser.parse_chunk().unwrap()
        };

        let closure_gc = {
            let mut global = lua.global.lock().unwrap();
            let globals = global.globals;
            let proto_gc = global.heap.allocate(proto);
            let uv = global.heap.allocate(crate::value::Upvalue { val: globals });
            global.heap.allocate(crate::value::Closure {
                proto: proto_gc,
                upvalues: vec![uv],
            })
        };

        lua.execute(closure_gc).await.unwrap();

        let global_val = {
            let mut global = lua.global.lock().unwrap();
            if let Value::Table(t) = global.globals {
                let res_key = Value::String(global.heap.allocate("res".to_string()));
                *t.map.get(&res_key).unwrap_or(&Value::Nil)
            } else {
                panic!("Globals is not a table");
            }
        };

        assert_eq!(global_val, Value::Integer(10));
    }
}
