# mlua-test (aarch64-musl-vendored-lua55)

This project is an example Rust application using `mlua` and `tokio`, configured
to link **statically** to vendored Lua 5.5 for `aarch64-unknown-linux-musl`
(target: OpenWrt).

## Prerequisites

1.  **Rust Target**:
    ```bash
    rustup target add aarch64-unknown-linux-musl
    ```
2.  **Zig Toolchain**: Required for cross-compilation. Download the pre-built
    binary for your platform from
    [ziglang.org/download](https://ziglang.org/download/) and add it to your
    `PATH`.
    - **Example (Linux x86_64)**:
      ```bash
      wget https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz
      tar -xf zig-linux-x86_64-0.13.0.tar.xz
      export PATH=$PATH:$(pwd)/zig-linux-x86_64-0.13.0
      ```

3.  **cargo-zigbuild**:
    ```bash
    cargo install cargo-zigbuild
    ```

## Build the App

```bash
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

The resulting binary will be located at:
`target/aarch64-unknown-linux-musl/release/mlua-test`

## Deployment (Installation)

To install the binary on your target device (e.g., OpenWrt):

1.  **Copy to Target**:
    ```bash
    scp target/aarch64-unknown-linux-musl/release/mlua-test root@<target-ip>:/usr/bin/
    ```
2.  **Set Permissions**:
    ```bash
    ssh root@<target-ip> "chmod +x /usr/bin/mlua-test"
    ```

## Runtime on OpenWrt

Since Lua 5.5 is vendored and linked statically, no external Lua library is
required on the target.

## Optimization Features

- **Size Optimization**:
  - `opt-level = "z"` (optimized for size)
  - `lto = true` (Link Time Optimization)
  - `panic = "abort"` (removes stack unwinding)
  - `strip = true` (removes symbols)
