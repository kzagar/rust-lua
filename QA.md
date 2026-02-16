# Quality Assurance (QA) Guide for rua

This document outlines the quality standards and procedures for the `rua` project.

## Development Workflow

To maintain high code quality, follow these steps before submitting any changes:

1.  **Code Formatting**: Use `cargo fmt` to ensure consistent code style.
    ```bash
    cargo fmt
    ```

2.  **Linting**: Run `clippy` to catch common mistakes and improve code quality.
    ```bash
    cargo clippy --all-targets --all-features -- -D warnings
    ```

3.  **Unit and Integration Testing**: Run all tests to ensure no regressions.
    ```bash
    cd rua
    cargo test
    ```

4.  **Lua Test Suite**: Run the comprehensive Lua test suite and verify the status of tests.
    ```bash
    cd rua
    cargo test --test lua_suite -- --nocapture
    ```
    - **Passed**: Tests that successfully execute.
    - **XFailed**: Expected failures for features not yet implemented.
    - **Failed**: Unexpected failures that must be addressed.
    - **XPassed**: Tests that were expected to fail but passed (the expectation should be updated).

## Test Runner Configuration

The Lua test suite runner is located in `rua/tests/lua_suite.rs`. It dynamically discovers tests in the `testes/` directory.

To update expectations for a test, modify the `get_expected_failures` function in `rua/tests/lua_suite.rs`.

## Documentation

- Keep `PLAN.md` updated as features are implemented.
- Update `DESIGN.md` if significant architectural changes are made.
