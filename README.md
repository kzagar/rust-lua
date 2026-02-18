# Lumen

A lightweight application framework. Small enough to run on home routers and
free-tier cloud virtual machines, yet powerful enough for hosting complex
applications.

Core features:

- **Small**. Low storage and memory usage.
- **Concurrent**. Uses async to andle multiple concurrent tasks efficiently.
- **Lua**. A lightweight, yet capable, scripting language for application logic.
- **Simple**. Easy to deploy, use, maintain and extend.
- **Zero maintenance**. Automated updating of the framework and applications.
- **Reliable**. Self-checks of updates, separate staging/shadow and production
  environments.

Optional features:

- **Web**. HTTP(S) server. Bind Lua functions to REST endpoints. Configure a
  reverse proxy. Serve static pages efficiently.
- **Authentication**. OAuth worflow for authentication.
- **Telemetry**. Logging and metrics - on local server or in the cloud.
- **Database**. Embedded SQLite database.
- **Scheduled tasks**. Cron-like scheduling.
- **Chat bot**. Notify humans about important events, and react to their
  requests.

Planned features:

- **Debugger**. Web interface into a running application.
- **AI**. Implement agent-based workflows, leveraging Large Language Models and
  MCP tools.
- **Speech**. Interact with apps using speech using text-to-speech and
  speech-to-text.
- **Documents**. Generate documents with LaTeX, PDF and Markdown support.

## What is an application?

An application is a collection of Lua scripts and configuration files that are
run by the Lumen framework.

# Lumen (aarch64-musl-vendored-lua55)

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
`target/aarch64-unknown-linux-musl/release/lumen`

## Deployment (Installation)

To install the binary on your target device (e.g., OpenWrt):

1.  **Copy to Target**:
    ```bash
    scp target/aarch64-unknown-linux-musl/release/lumen root@<target-ip>:/usr/bin/
    ```
2.  **Set Permissions**:
    ```bash
    ssh root@<target-ip> "chmod +x /usr/bin/lumen"
    ```

## Runtime on OpenWrt

Since Lua 5.5 is vendored and linked statically, no external Lua library is
required on the target.

## Examples

Example Lua scripts are located in the `examples/` directory:

- `examples/example.lua`: Basic script demonstrating core functionality.
- `examples/rest-server.lua`: A REST server with multiple endpoints.
- `examples/rest-client.lua`: Using the HTTP client to fetch data.
- `examples/gmail.lua`: Gmail and Google Drive integration.
- `examples/proxy.lua`: Reverse proxy with authentication.
- `examples/telegram.lua`: Telegram bot integration.

To run an example:

```bash
cargo run -- examples/example.lua
```

## Gmail Support

To enable Gmail support, you need to set up Google Cloud Platform (GCP)
credentials:

1.  **GCP Setup**:
    - Go to the [Google Cloud Console](https://console.cloud.google.com/).
    - Create a new project or select an existing one.
    - Navigate to **APIs & Services > Library** and enable the **Gmail API** and
      **Google Drive API**.
    - Go to **APIs & Services > OAuth consent screen**:
      - Select **External**.
      - Add yourself as a test user.
      - Add scopes: `https://www.googleapis.com/auth/gmail.modify`,
        `https://www.googleapis.com/auth/gmail.compose`, and
        `https://www.googleapis.com/auth/drive`.
    - Go to **APIs & Services > Credentials**:
      - Click **Create Credentials > OAuth client ID**.
      - Select **Web application**.
      - Under **Authorized redirect URIs**, add:
        `https://localhost:3443/auth/google/callback` (adjust the port if you've
        changed it in your Lua script).
      - Click **Create** and then **Download JSON**.

2.  **Configuration**:
    - Create a file named `.secrets` in the project root (formatted as a `.env`
      file).
    - Add the following variables:

      ```dotenv
      GOOGLE_CLIENT_SECRET=/path/to/your/client_secret.json
      GMAIL_ATTACHMENT_DIR=attachments  # Optional
      ```

3.  **TLS Certificates**:
    - By default, the server expects `cert.pem` and `key.pem` in the root
      directory for HTTPS. You can generate them for testing:
      ```bash
      openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -sha256 -days 3650 -nodes -subj "/C=XX/ST=State/L=City/O=Org/OU=Unit/CN=localhost"
      ```

## Logging

Logging can be tuned using the `RUST_LOG` environment variable. By default, the
log level is set to `info`.

- **Set Log Level**:

  ```bash
  export RUST_LOG=debug
  cargo run -- examples/example.lua
  ```

- **Available Levels**: `error`, `warn`, `info`, `debug`, `trace`.

- **Module Specific Logging**:
  ```bash
  export RUST_LOG=lumen=debug,ureq=warn
  ```

## Optimization Features

- **Size Optimization**:
  - `opt-level = "z"` (optimized for size)
  - `lto = true` (Link Time Optimization)
  - `panic = "abort"` (removes stack unwinding)
  - `strip = true` (removes symbols)

## Run Concurrency Test

To run the concurrency test:

1. **Start the server**:

   ```bash
   cargo run -- tests/concurrency-server.lua
   ```

2. **Run the test script** using `uv`:

```bash
uv run tests/concurrency.py
```

This will automatically manage Python dependencies (`httpx`, `numpy`,
`matplotlib`, `tabulate`) using the configuration in `pyproject.toml`.
