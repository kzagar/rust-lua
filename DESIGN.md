# Design Document: Rua - Async Lua for Web Services

## 1. Introduction
Rua is a lightweight Lua interpreter written in Rust, designed around the async paradigm. Its primary goal is to facilitate the development of web server applications that demand low memory footprint and low serving latency.

By leveraging Rust's safety and performance, along with native `async/await` support, Rua provides an environment where Lua scripts can efficiently handle high-concurrency workloads typical of modern web services.

## 2. Vision and Core Goals
- **Low Latency & Low Memory**: Optimized for high-performance serving environments.
- **Async-First**: The VM is natively async-aware, allowing non-blocking I/O and seamless integration with the Rust async ecosystem.
- **Web-Ready**: Built-in support (via add-on crates) for common web server requirements like REST HTTP(S), SQLite, and JSON parsing.
- **Lua 5.5 Compatibility**: Target compatibility with the latest Lua specifications while providing modern extensions (e.g., `global` keyword).

## 3. Ecosystem Architecture
Rua is organized as a modular set of crates to keep the core interpreter lightweight while providing rich functionality through extensions.

- **`rua`**: The core Lua interpreter and VM.
- **`rua-resty-http`**: Provides a Lua API for making asynchronous HTTP(S) requests, modeled after `lua-resty-http`.
- **`rua-sqlite`** (Planned): Asynchronous SQLite integration for persistent storage.
- **`rua-axum`** (Planned): Integration with the Axum web framework, allowing Rua functions to be bound directly to REST endpoints.
- **`rua-web`** (Planned): A comprehensive web server package that combines `rua`, `rua-axum`, `rua-resty-http`, and `rua-sqlite` on top of the Tokio runtime.
- **`rua-cli`**: A command-line interface for executing Rua scripts, including support for HTTP and SQLite integrations via Tokio.

## 4. Performance Drivers
### 4.1 Native Rust Primitives
To minimize latency, common web server operations are implemented as native Rust primitives and exposed to Lua. This includes:
- Asynchronous HTTP(S) requests.
- SQLite database interactions.
- JSON parsing and serialization.
By keeping these performance-critical tasks in Rust, Rua minimizes the overhead of executing complex logic within the Lua VM.

### 4.2 Async VM Loop
The core VM loop is implemented as an `async` function. This allows the VM to yield execution naturally when waiting for I/O, without blocking host threads.

### 4.3 Future JIT Optimization
A future phase of development will introduce a Just-In-Time (JIT) compiler based on LuaJIT principles to further enhance execution speed for hot code paths.

## 5. Benchmarking and Quality Assurance
To ensure the goals of low memory and latency are met, a suite of benchmarks will be maintained:
- **Characteristic Workloads**: Benchmarks simulating typical web server tasks (e.g., API requests with database lookups).
- **Comparative Analysis**: Performance comparisons against standard Lua (C implementation) and Python's FastAPI to validate Rua's advantages in web serving scenarios.
- **Regression Testing**: Automated performance tracking to catch latency or memory usage regressions during development.

## 6. Core Architecture

### 6.1 LuaState and GlobalState
- **`LuaState`**: Represents a Lua thread (coroutine). It contains its own execution stack and a stack of `CallFrame`s. It is `Send` but not `Sync`, allowing it to be moved between threads (e.g., across `tokio::spawn` points).
- **`GlobalState`**: Contains data shared across all `LuaState` instances within the same environment, such as the string table, global environment, and the Garbage Collector (GC) heap.

### 6.2 Call Frames
The VM uses a call stack of `CallFrame`s to manage function execution.
- **`CallFrame`**: Stores the active `Closure`, the program counter (`pc`), the stack `base` (index where local registers start), and the number of expected results (`nresults`).
- This architecture allows for non-recursive Lua-to-Lua calls and proper register isolation.

### 6.3 Value Representation
Lua values are represented by an idiomatic Rust `enum`:
```rust
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
```

## 7. Garbage Collection
A custom mark-and-sweep garbage collector is used.
- **`Gc<T>`**: A smart pointer that tracks references to objects in the GC heap.
- **Tracing**: Objects implement a `Trace` trait to allow the GC to find reachable objects.

## 8. Async VM Execution
The core VM loop is an `async` function:
```rust
impl LuaState {
    pub async fn execute(&mut self) -> Result<(), LuaError> {
        while let Some(instruction) = self.fetch_instruction() {
            self.dispatch(instruction).await?;
        }
        Ok(())
    }
}
```

### 8.1 Async Callbacks
Rust functions exposed to Lua can be `async fn`. When Lua calls such a function, the VM will `await` its completion.
```rust
type AsyncCallback = Box<dyn for<'a> Fn(&'a mut LuaState) -> BoxFuture<'a, Result<int, LuaError>> + Send>;
```

## 9. The Parser and Compiler
A robust recursive descent parser is implemented, matching Lua 5.4's expression precedence and statement structure.
- **Supported Syntax**: Includes local/global declarations with attributes (`<const>`, `<close>`), explicit `global` declarations, and all standard Lua arithmetic, bitwise, and relational operations.
- **Compiler**: Generates Lua 5.4 compatible 32-bit instructions.

## 10. VM Instruction Set
The VM implements a subset of Lua 5.4 opcodes with the exact bit layout:
- `iABC`, `iABx`, `iAsBx`, `iAx`, `isJ` formats.
- Supports immediate operands via the `k` bit and specialized opcodes like `LOADI`.

## 11. Variables and Scoping
- **Local Variables**: Stored on the VM stack.
- **Global Variables**: Handled via the `_ENV` upvalue, which points to the global table.
- **Upvalues**: Support for mutable shared upvalues is implemented.

## 12. Error Handling
Rua uses Rust's `Result<T, LuaError>` for error handling instead of C-style `longjmp`, ensuring safety and proper stack unwinding.

## 13. Current Simplifications and Limitations
- **Tables**: Currently implemented using Rust's `HashMap<Value, Value>`.
- **Upvalues**: "Open" upvalues that track live stack slots are not yet implemented.
- **Metatables**: Basic support for `__index` metamethod is implemented for Tables and UserData.
- **String Table**: Strings are GC-allocated but not yet internalized in a global table.

## 14. IO and Standard Library
IO-bound functions are implemented using `tokio`'s async IO traits to maintain the non-blocking nature of the VM.
