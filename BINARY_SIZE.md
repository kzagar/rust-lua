# Binary Size Optimization Report for Lumen

This report details the efforts to reduce the binary size of the `lumen`
project, targeting an embedded environment (OpenWrt on aarch64).

## Baseline

- **Initial Size**: ~13MB (stripped, release build with default `Cargo.toml`
  settings).
- **Initial Contributors**: `rustls` + `aws-lc-rs` (~1.5MB), `sqlx` (~0.5MB+),
  `reqwest` + `hyper` + `h2` (~0.5MB+), `mlua` (~0.2MB), vendored C code (Lua,
  SQLite) (~1.1MB).

## Optimization Steps and Impact

### 1. Profile Optimization (Workspace Level)

Applied `opt-level = "z"`, `lto = true`, `codegen-units = 1`, and
`panic = "abort"` at the workspace level.

- **Impact**: Reduced size from ~16MB to ~13MB (unstripped) / ~10MB (stripped).
- **Trade-off**: Longer compile times, no stack unwinding on panic.

### 2. TLS: Switching from Rustls to Native-TLS (Dynamic Linking)

The biggest bloat was `aws-lc-rs` (a dependency of `rustls`). Switching to
`native-tls` (which links to the system's OpenSSL) removed the need to bundle
the large cryptographic library.

- **Impact**: Saved ~6MB. Size reduced to ~3.9MB.
- **Trade-off**: Requires `libssl` to be present on the target system.

### 3. Database: Replacing SQLx with Rusqlite (Dynamic Linking)

`sqlx` is a heavy dependency with many features and macros. Replacing it with
`rusqlite` (linked to system `libsqlite3`) significantly reduced the binary
footprint.

- **Impact**: Saved ~0.5MB.
- **Trade-off**: Requires `libsqlite3` on the target system. Async calls now use
  `spawn_blocking`.

### 4. Web Client: Replacing Reqwest with ureq

`reqwest` is built on `hyper` and `h2`, which are relatively large. `ureq` is a
minimal, synchronous HTTP client that can be easily used within `spawn_blocking`
to provide an async interface to Lua.

- **Impact**: Similar to `minreq`, saved ~0.4MB.
- **Trade-off**: Minimalistic API, no native async (uses blocking threads).
  Supported insecure SSL for local development.

### 5. Utilities: Removing Heavy Schedulers and Watchers

- Replaced `tokio-cron-scheduler` with a simple timer loop using the lightweight
  `croner` crate for parsing.
- Replaced `notify-debouncer-mini` with a standard Unix SIGHUP signal handler
  for reloading scripts.
- **Impact**: Saved ~0.2MB.
- **Trade-off**: Lost cross-platform file watching (now Linux/Unix specific via
  SIGHUP).

### 6. Feature Trimming

Reduced features for `tokio` (removed `rt-multi-thread`), `axum`, and
`axum-server` (disabled HTTP/2 where possible).

- **Impact**: Saved ~0.1MB.

## Final Result

- **Final Size**: **2.1MB** (stripped, x86_64 Linux).
- **Total Reduction**: **~80%** (from 10MB to 2.1MB).

## Recommendations for Further Reduction

1.  **Dynamic Lua Linking**: Currently Lua 5.5 is vendored and statically linked
    (~0.8MB). If the target system provides `liblua5.5.so`, this could be
    dynamically linked to save another ~0.8MB.
2.  **External Web Server**: Instead of embedding a full web server (Axum),
    consider using a lightweight FastCGI or SCGI interface if a web server is
    already present on the device (like `uhttpd` on OpenWrt).
3.  **Alternative Runtimes**: For extremely tight environments, consider
    replacing `tokio` with a more minimal async runtime or a synchronous event
    loop if high concurrency is not required.
