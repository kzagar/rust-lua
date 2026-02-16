# Improvement Plan for Rust Lua (rua)

This plan outlines the steps to improve `rua` to cover an increasing amount of the Lua 5.5 test suite.

## Phase 1: Infrastructure and Core Language Constructs
- [x] Create a test runner to execute `testes/` files and report status.
- [x] Implement `global` keyword and basic global declarations.
- [x] Support `<const>` and `<close>` attributes.
- [x] Handle `global <const> *` syntax.

## Phase 2: String Manipulation
- [x] Implement missing `string` library functions required by `strings.lua` and `literals.lua`.
- [x] Improve string escape sequence handling in the Lexer (e.g., `\z`, hexadecimal/Unicode escapes).
- [x] Support long brackets `[[ ... ]]` and comments properly.

## Phase 3: Table Behavior
- [ ] Implement metamethods beyond `__index` (e.g., `__newindex`, `__call`, `__len`).
- [ ] Improve Table implementation to support more Lua-like behavior if needed.

## Phase 4: Garbage Collection
- [ ] Enhance the mark-and-sweep GC.
- [ ] Implement `collectgarbage` standard library function.

## Phase 5: Advanced Features and Full Suite Coverage
- [ ] Implement coroutines and `coroutine` library.
- [ ] Implement `debug` library.
- [ ] Finalize remaining standard library functions.

## Phase 6: Language Core Refinement
- [ ] Support multiple assignment for table fields and indices.
- [ ] Implement `break` and `goto` statements fully.
- [ ] Support Lua patterns in `string` library.
- [ ] Implement full Lua 5.4/5.5 standard library compatibility.

## Phase 7: Performance and Optimization
- [ ] Optimize table lookups using hybrid array/hash representation.
- [ ] Implement basic JIT compilation for hot loops.
- [ ] Improve garbage collector performance (incremental/generational).
