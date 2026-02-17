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

## Optimization Features

- **Size Optimization**:
  - `opt-level = "z"` (optimized for size)
  - `lto = true` (Link Time Optimization)
  - `panic = "abort"` (removes stack unwinding)
  - `strip = true` (removes symbols)

## Run Concurrency Test

To run the concurrency test using `uv`:

```bash
uv run python3 concurrency_test.py
```

This will automatically manage Python dependencies (`httpx`, `numpy`,
`matplotlib`, `tabulate`) using the configuration in `pyproject.toml`.
