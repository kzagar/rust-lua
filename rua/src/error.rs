use thiserror::Error;

#[derive(Error, Debug)]
pub enum LuaError {
    #[error("syntax error: {0}")]
    SyntaxError(String),
    #[error("runtime error: {0}")]
    RuntimeError(String),
    #[error("memory error")]
    MemoryError,
}
