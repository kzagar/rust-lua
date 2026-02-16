# Design Document: Rua LLM Integration (rua-llm)

This document outlines the design and implementation of the LLM integration for Rua, provided by the `rua-llm` module. The integration is powered by `swarms-rs`, an enterprise-grade multi-agent orchestration framework for Rust.

## 1. Overview

`rua-llm` aims to bring advanced AI capabilities to the Rua ecosystem. By integrating `swarms-rs`, Rua users can build sophisticated multi-agent systems, leverage various LLM providers, and use Model Context Protocol (MCP) tools directly from Lua.

The module is designed to be async-first, ensuring that LLM calls do not block the VM execution loop.

## 2. Lua API Design

The `rua-llm` module is registered under the global `llm` table. It follows an idiomatic Lua table-based configuration style.

### 2.1 LLM Providers

Providers are configured as tables within agent or workflow constructors.

```lua
local openai_config = {
    type = "openai",
    api_key = "sk-...",
    model = "gpt-4o",
    base_url = "https://api.openai.com/v1" -- Optional
}

local deepseek_config = {
    type = "openai", -- swarms-rs uses OpenAI provider for DeepSeek
    api_key = "sk-...",
    model = "deepseek-chat",
    base_url = "https://api.deepseek.com/v1"
}
```

### 2.2 Agents

Agents are the primary building blocks. They wrap a language model with a system prompt, memory, and tools.

#### `llm.agent.new(config)`
Creates a new Agent.

**Config table fields:**
- `name` (string): The name of the agent.
- `provider` (table): Provider configuration.
- `system_prompt` (string): Instructions for the LLM.
- `user_name` (string, optional): Default name for the user.
- `max_loops` (integer, optional): Maximum autonomous loops (default: 1).
- `temperature` (number, optional): LLM temperature.
- `tools` (list of tables, optional): List of MCP tools.
- `stop_words` (list of strings, optional): Custom stop sequences.
- `autosave` (boolean or table, optional): Configuration for state persistence.

**Methods:**
- `agent:run(input)`: Executes the agent with the given input. Returns `(response, nil)` on success, or `(nil, error_message)` on failure.

**Example:**
```lua
local agent = llm.agent.new({
    name = "Assistant",
    provider = deepseek_config,
    system_prompt = "You are a helpful assistant.",
    max_loops = 1
})

local res, err = agent:run("Hello, what is Rua?")
if err then
    print("Error:", err)
else
    print("Response:", res)
end
```

### 2.3 Multi-Agent Workflows

`swarms-rs` supports various architectures for agent collaboration.

#### `llm.workflow.concurrent(config)`
Executes multiple agents in parallel.

**Config table fields:**
- `name` (string): Workflow name.
- `agents` (list of Agents): The agents to run.
- `description` (string, optional): Description of the task.

#### `llm.workflow.sequential(config)`
Executes agents in a linear sequence.

**Methods:**
- `workflow:run(input)`: Returns `(json_result, nil)` or `(nil, error_message)`.

### 2.4 MCP Tools

Agents can be equipped with tools following the Model Context Protocol.

```lua
local agent = llm.agent.new({
    -- ...
    tools = {
        {
            type = "stdio",
            command = "uvx",
            args = {"mcp-hn"}
        },
        {
            type = "sse",
            name = "filesystem",
            url = "http://localhost:8000/sse"
        }
    }
})
```

## 3. Implementation Details

### 3.1 Data Structures

The `rua-llm` crate will define `UserData` wrappers for `swarms-rs` types:

```rust
pub struct LuaAgent {
    pub inner: swarms_rs::structs::agent::Agent,
}

impl LuaUserData for LuaAgent {
    // ... trait implementation
}
```

### 3.2 Async Integration

LLM calls are I/O bound and potentially long-running. `rua-llm` uses `AsyncCallback` to ensure the VM yields during these calls.

```rust
fn agent_run(state: &mut LuaState) -> BoxFuture<'_, Result<usize, LuaError>> {
    async move {
        // 1. Extract agent from UserData
        // 2. Extract input string from stack
        // 3. Call agent.run(input).await
        // 4. Push (result, nil) or (nil, error) to stack
        Ok(n_results)
    }.boxed()
}
```

### 3.3 Mapping Lua Tables to Rust Builders

`swarms-rs` uses a builder pattern for configuration. `rua-llm` will implement a mapping layer that traverses Lua tables and applies them to the builders.

```rust
fn create_agent(config: &Table) -> Result<Agent, String> {
    let mut builder = OpenAI::new(api_key).agent_builder();

    if let Some(name) = config.get("name") {
        builder = builder.agent_name(name);
    }
    // ...
    Ok(builder.build())
}
```

### 3.4 Garbage Collection

`LuaAgent` and `LuaWorkflow` structs must implement `GCTrace`. Since these structs primarily contain Rust-native data (strings, network clients) and do not hold `Gc<T>` pointers back into the Lua heap (unless we support Lua-defined tools in the future), `trace` will typically be an empty implementation.

### 3.5 Error Handling

In alignment with Lua conventions, LLM operations will return a result and an error message rather than throwing exceptions, especially since LLM failures (timeouts, rate limits) are expected runtime conditions.

```lua
local ok, err = agent:run("...")
if not ok then
    -- Handle err
end
```

## 4. Performance Considerations

### 4.1 Native Tool Execution
MCP tools are executed as separate processes (STDIO) or via HTTP (SSE). Rua's async nature allows multiple agents with tools to run concurrently without blocking the main event loop.

### 4.2 Resource Management
LLM agents can hold significant state in memory (conversation history). Users should be aware of `max_loops` and memory growth. Rua's GC will manage the lifecycle of `Agent` objects.
