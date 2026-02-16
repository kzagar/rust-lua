# Design Document: Rua Parser Evolution

This document evaluates the current implementation of the Rua parser and explores various alternatives for its future development, focusing on maintainability, performance, and high-quality syntax error reporting.

## 1. Current State: Hand-written Recursive Descent

The current parser in `rua/src/parser.rs` is a hand-written recursive descent parser that performs a single-pass compilation from source code directly to Lua VM bytecode.

### Pros
- **Zero Dependencies:** No external crates are required for the core parsing logic.
- **Performance:** One-pass compilation is traditionally the fastest way to compile Lua.
- **Control:** Full control over how every character is handled, allowing for custom Lua-specific optimizations during parsing.
- **Minimal Memory Overhead:** No intermediate AST is built, reducing memory pressure.

### Cons
- **Maintenance Burden:** As more Lua 5.5 features are added (complex loops, table constructors, etc.), the manual management of tokens and state becomes increasingly error-prone.
- **Poor Error Reporting:** Currently, errors are just strings. Adding precise line/column numbers, spans, and helpful snippets requires significant manual effort and boilerplate in the Lexer and Parser.
- **Hard to Refactor:** Changes to the grammar require manual updates to nested function calls, which can lead to subtle bugs.

## 2. Option 1: Evolve the Hand-written Parser

We could keep the current architecture but modernize it by introducing a `Span` type and using a library like `ariadne` or `codespan-reporting` for error visualization.

- **Strategy:** Update `Lexer` to track offsets and `Token` to include `Span`.
- **Error Reporting:** Use `ariadne` to print beautiful, clustered error messages when a `SyntaxError` occurs.
- **Maintainability:** Moderate. Requires discipline but keeps the "one-pass" philosophy.

## 3. Option 2: Parser Combinators (`winnow` / `nom`)

Libraries like `winnow` (a faster, more user-friendly fork of `nom`) allow for building parsers by composing small functions.

### Pros
- **Type Safety:** Strongly typed parsers.
- **Composability:** Easy to test small parts of the grammar in isolation.
- **Modern Rust:** Very idiomatic for the Rust ecosystem.

### Cons
- **Error Messages:** While `winnow` has improved error reporting, it still often requires significant effort (and extra crates like `winnow-locate`) to provide the level of detail a hand-written or PEG parser can offer.
- **Complexity:** Complex recursion and back-tracking in Lua's grammar can lead to "combinator soup" that is hard to debug.

## 4. Option 3: PEG-based Parsers (`pest`)

`pest` uses a separate `.pest` file to define a Parsing Expression Grammar (PEG).

### Pros
- **Formal Grammar:** The grammar is defined clearly in one place, making it easy to see the "truth" of the language syntax.
- **Excellent Error Reporting:** `pest` provides top-tier error messages out-of-the-box, including source snippets and pointing exactly where the error occurred.
- **Maintainability:** High. Adding new Lua 5.5 syntax is often as simple as updating the grammar file.

### Cons
- **Mandatory AST:** `pest` produces a "CST" (Concrete Syntax Tree) which you *must* iterate over to build an AST or emit bytecode. This forces a two-pass architecture.
- **Performance:** Slower than a hand-written one-pass parser due to the intermediate tree and the nature of PEG parsing.

## 5. Option 4: Formal Grammar Generators (`LALRPOP`)

`LALRPOP` is a popular LR(1) parser generator for Rust.

### Pros
- **Powerful:** Can handle more complex grammars than PEG in some cases.
- **Performance:** Generally faster than PEG.
- **Strong Typing:** Generates Rust code that is fully integrated into the project.

### Cons
- **Learning Curve:** LALRPOP's DSL and the concepts of LR(1) parsing are more complex.
- **Error Reporting:** Good, but often requires manual configuration to match the "friendly" output of `pest` or a well-tuned hand-written parser.

## 6. Architectural Choice: One-pass vs. Two-pass (AST)

Standard Lua (C) is famous for its one-pass compiler. However, for a modern Rust implementation:

- **One-pass:** Fits Lua's history. Harder to implement complex optimizations (like register allocation or constant folding across multiple statements).
- **Two-pass (AST):**
  - Allows for a **Lossless AST**, which can power future tooling (formatters, linters, LSP).
  - Enables advanced optimizations before bytecode emission.
  - Easier to decouple syntax analysis from code generation.

## 7. Syntax Error Reporting

High-quality error reporting is a priority for Rua. A good error message should look like this:

```text
error: unexpected token
  --> main.lua:2:15
   |
 2 | local x = 10 +
   |               ^ expected expression
```

To achieve this:
1. **Spans:** Every token and AST node must track its byte range in the source.
2. **Context:** The parser needs to maintain enough state to explain *what* it was expecting.
3. **Visualization:** Use a library like `ariadne` to render the final error message.

## 8. Summary Comparison

| Feature | Hand-written | winnow | pest | LALRPOP |
| :--- | :--- | :--- | :--- | :--- |
| **Performance** | Excellent | Very Good | Good | Very Good |
| **Error Reporting** | Manual (Hard) | Moderate | Excellent (Easy) | Good |
| **Maintainability** | Low | Moderate | High | High |
| **One-pass Support** | Native | Possible | No | No |
| **Dependency Weight** | None | Low | Medium | High |

## 9. Recommendation

For `rua`, I recommend a **Hybrid Approach**:

1. **Adopt a Two-pass Architecture:** Build an AST first. This will simplify the implementation of Lua 5.5's evolving syntax and allow for future optimizations.
2. **Use `logos` for Lexing:** It is the fastest lexer in the Rust ecosystem and integrates easily with other tools.
3. **Choice of Parser:**
   - **Option A (The "Safe" Path):** Use **`pest`** if the absolute priority is getting the project running with excellent error messages quickly. The separate grammar file will be very helpful as Lua 5.5 evolves.
   - **Option B (The "Power" Path):** Stick with a **Hand-written Parser** but refactor it to produce an AST and use **`ariadne`** for errors. This retains the "Lua spirit" while fixing the main pain points.

**Immediate Next Steps:**
- Define an `Ast` enum in `rua/src/ast.rs`.
- Introduce a `Span` struct to track source locations.
- Decide between `pest` (formal grammar) and hand-written AST (manual control) based on team preference for external dependencies.
