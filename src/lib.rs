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
                Instruction(2 | (2 << 7) | (0 << 16) | (1 << 24)), // ADD R[2] R[0] R[1]
            ],
            k: vec![Value::Integer(10), Value::Integer(20)],
        };
        let proto_gc = {
            let mut global = lua.global.lock().unwrap();
            global.heap.allocate(proto)
        };
        lua.execute(proto_gc).await.unwrap();
        if let Value::Integer(res) = lua.stack[2] {
            assert_eq!(res, 30);
        } else {
            panic!("Result is not an integer");
        }
    }
}
