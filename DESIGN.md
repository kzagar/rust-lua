# Design Document: Async Lua Port to Rust

## 1. Introduction
This document outlines the design for porting the Lua VM to Rust with native `async/await` support. The goal is to provide a Lua execution environment where the VM itself is async-aware, allowing Lua scripts to call async Rust functions and yield execution naturally.

## 2. Core Architecture

### 2.1 LuaState and GlobalState
- **`LuaState`**: Represents a Lua thread (coroutine). It contains its own execution stack and call information. It will be `Send` but not `Sync`.
- **`GlobalState`**: Contains data shared across all `LuaState` instances within the same environment, such as the string table, global environment, and the Garbage Collector (GC) heap.

### 2.2 Value Representation
Lua values will be represented by an idiomatic Rust `enum`:
```rust
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(Gc<String>),
    Table(Gc<Table>),
    Function(Gc<Closure>),
    UserData(Gc<Box<dyn Any + Send>>),
    Thread(Gc<LuaState>),
}
```

## 3. Garbage Collection
A custom mark-and-sweep garbage collector will be implemented.
- **`Gc<T>`**: A smart pointer that tracks references to objects in the GC heap.
- **Heap Management**: The `GlobalState` will manage an arena or a collection of allocated objects.
- **Tracing**: Objects will implement a `Trace` trait to allow the GC to find reachable objects.

## 4. Async VM Execution
The core VM loop will be an `async` function, allowing it to `.await` on any operation.
```rust
impl LuaState {
    pub async fn execute(&mut self) -> Result<(), LuaError> {
        while let Some(instruction) = self.fetch_instruction() {
            self.dispatch(instruction).await?;
        }
        Ok(())
    }

    async fn dispatch(&mut self, instruction: Instruction) -> Result<(), LuaError> {
        match instruction.opcode() {
            OpCode::Call => self.call_function().await,
            // ... other opcodes
        }
    }
}
```

### 4.1 Async Callbacks
Rust functions exposed to Lua can be `async fn`. When Lua calls such a function, the VM will `await` its completion.
```rust
type AsyncCallback = Box<dyn for<'a> Fn(&'a mut LuaState) -> BoxFuture<'a, Result<int, LuaError>> + Send>;
```

## 5. The Parser and Compiler
A robust recursive descent parser has been implemented, matching Lua 5.4's expression precedence and statement structure.
- **Supported Syntax**:
    - Local variable declarations (`local x = 10`).
    - Assignments (local and global).
    - Arithmetic operations: `+`, `-`, `*`, `/`, `//`, `%`, `^`, unary `-`.
    - Bitwise operations: `&`, `|`, `~`, `<<`, `>>`, unary `~`.
    - Relational operations: `==`, `~=`, `<`, `>`, `<=`, `>=`.
    - Logical operations: `not` (initial support, `and`/`or` via expression precedence).
    - Length operator `#` and concatenation `..`.
    - Nested scopes using `do ... end`.
    - Function calls as statements.
- **Compiler**: Generates Lua 5.4 compatible 32-bit instructions. Constants are stored in the prototype's constant table (`k`).

## 6. VM Instruction Set
The VM now implements a subset of Lua 5.4 opcodes with the exact bit layout:
- `OpCode` (7 bits), `A` (8 bits), `C` (9 bits), `B` (8 bits) for `iABC`.
- `Bx` (17 bits) for `iABx`, `sBx` for `iAsBx`.
- Supports immediate operands via the `k` bit and specialized opcodes like `LOADI`.

## 7. Variables and Scoping
- **Local Variables**: Stored on the VM stack. The compiler tracks register allocation and scope depth.
- **Global Variables**: Handled via the `_ENV` upvalue, which points to the global table.
- **Upvalues**: Initial support for upvalues in `Closure` objects.

## 8. Current Simplifications and Limitations
- **Tables**: Currently implemented using Rust's `HashMap<Value, Value>`. Does not yet feature the dual array/hash representation of standard Lua.
- **Upvalues**: Simplified version; "open" upvalues (pointing to live stack slots) are not yet implemented. All upvalues are currently "closed".
- **Metatables**: Not yet implemented.
- **String Table**: Strings are currently allocated in the GC heap but not internalized in a global string table.
- **Function Calls**: Lua-to-Lua calls are implemented recursively in the `execute` function.

## 9. Error Handling
Instead of C-style `longjmp`, the entire codebase will use Rust's `Result<T, LuaError>`. This ensures safety and proper stack unwinding.

## 10. Concurrency Model
- `LuaState` is `Send`, allowing it to be moved between threads (e.g., across `tokio::spawn` points).
- It is not `Sync`, as Lua execution is inherently single-threaded per state.
- Multiple `LuaState`s can share a `GlobalState` if protected by appropriate synchronization, though initially, we may target a single-threaded execution model for the `GlobalState` as well (e.g., using `Rc` and `RefCell` internally if confined to one thread, or `Arc` and `Mutex` if shared).

## 11. IO and Standard Library
IO-bound functions (like `print`, `io.read`, etc.) will be implemented using `tokio`'s async IO traits.

## 12. Implementation Plan
1. Define core types (`Value`, `Instruction`, `LuaError`).
2. Implement the basic GC infrastructure.
3. Implement the VM execution loop with a few basic opcodes.
4. Implement a subset of the parser/compiler.
5. Integrate `tokio` for async IO callbacks.
