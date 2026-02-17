#!/bin/bash
set -e

# Configuration
PKG_NAME="mlua-test"
PKG_VERSION="0.1.0-1"
PKG_ARCH="aarch64_cortex-a53"
TARGET="aarch64-unknown-linux-musl"
ZIG_PATH="/home/kzagar/rust-lua/zig-toolchain/zig-linux-x86_64-0.13.0"

# Add zig to PATH
export PATH="$PATH:$ZIG_PATH"

echo "### Building binary for $TARGET ###"
cargo zigbuild --release --target "$TARGET"

# Prepare staging directory
STAGING_DIR="target/opkg_staging"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/etc/mlua-test"
mkdir -p "$STAGING_DIR/etc/init.d"
mkdir -p "$STAGING_DIR/CONTROL"

# Copy binary
# Note: since this is a workspace, the target directory is in the parent folder
BINARY_PATH="../target/$TARGET/release/$PKG_NAME"
if [ ! -f "$BINARY_PATH" ]; then
    # Try local target if not in workspace
    BINARY_PATH="target/$TARGET/release/$PKG_NAME"
fi

if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Binary not found at $BINARY_PATH"
    exit 1
fi

cp "$BINARY_PATH" "$STAGING_DIR/usr/bin/"

# Copy Lua scripts and assets to /etc/mlua-test
cp example-rest-server.lua "$STAGING_DIR/etc/mlua-test/"
cp cert.pem key.pem "$STAGING_DIR/etc/mlua-test/"

# Copy public directory
if [ -d "public" ]; then
    cp -r public "$STAGING_DIR/etc/mlua-test/"
fi

# Create init script
cat <<EOF > "$STAGING_DIR/etc/init.d/mlua-test"
#!/bin/sh /etc/rc.common

START=99
USE_PROCD=1

start_service() {
    procd_open_instance
    procd_set_param command /usr/bin/mlua-test /etc/mlua-test/example-rest-server.lua
    procd_set_param stdout 1
    procd_set_param stderr 1
    # Run in /etc/mlua-test so relative paths like server.db work
    procd_set_param pwd /etc/mlua-test
    procd_close_instance
}
EOF
chmod +x "$STAGING_DIR/etc/init.d/mlua-test"

# Create control file
cat <<EOF > "$STAGING_DIR/CONTROL/control"
Package: $PKG_NAME
Version: $PKG_VERSION
Architecture: $PKG_ARCH
Maintainer: Antigravity
Section: utils
Priority: optional
Description: mlua-test application with Lua 5.5 and SQLite3
Source: https://github.com/kzagar/rust-lua
EOF

# Create conffiles to preserve configuration and database on upgrade
cat <<EOF > "$STAGING_DIR/CONTROL/conffiles"
/etc/mlua-test/example-rest-server.lua
/etc/mlua-test/cert.pem
/etc/mlua-test/key.pem
/etc/mlua-test/server.db
EOF

# Create postinst script
cat <<EOF > "$STAGING_DIR/CONTROL/postinst"
#!/bin/sh
[ -z "\$IPKG_INSTROOT" ] && {
    chmod +x /etc/init.d/mlua-test
    /etc/init.d/mlua-test enable
    /etc/init.d/mlua-test start
}
exit 0
EOF
chmod +x "$STAGING_DIR/CONTROL/postinst"

# Packaging
PKGOUT_DIR="target/opkg_out"
rm -rf "$PKGOUT_DIR"
mkdir -p "$PKGOUT_DIR"

echo "### Packaging $PKG_NAME ###"

# 1. Create control.tar.gz
(
    cd "$STAGING_DIR/CONTROL"
    tar czf "../../opkg_out/control.tar.gz" .
)

# 2. Create data.tar.gz
(
    cd "$STAGING_DIR"
    tar czf "../opkg_out/data.tar.gz" --exclude CONTROL .
)

# 3. Create debian-binary
echo "2.0" > "$PKGOUT_DIR/debian-binary"

# 4. Combine into final .ipk (using ar)
PKG_FILE="${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.ipk"
(
    cd "$PKGOUT_DIR"
    # Note: the order matters for some old opkg versions
    ar r "../../$PKG_FILE" debian-binary control.tar.gz data.tar.gz
)

echo "### Done! Package is at $PKG_FILE ###"
