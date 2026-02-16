# Improvement Plan for Rust Lua (rua)

This plan outlines the steps to improve `rua` to cover an increasing amount of the Lua 5.5 test suite.

## Phase 1: Infrastructure and Core Language Constructs
- [ ] Create a test runner to execute `testes/` files and report status.
- [ ] Implement `global` keyword and basic global declarations.
- [ ] Support `<const>` and `<close>` attributes.
- [ ] Handle `global <const> *` syntax.

## Phase 2: String Manipulation
- [ ] Implement missing `string` library functions required by `strings.lua` and `literals.lua`.
- [ ] Improve string escape sequence handling in the Lexer (e.g., `\z`, hexadecimal/Unicode escapes).
- [ ] Support long brackets `[[ ... ]]` and comments properly.

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
