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

echo "### Building binary for $TARGET (PROD) ###"
MLUA_TEST_BUILD_ENV=prod cargo zigbuild --release --target "$TARGET"

# Prepare staging directory
STAGING_DIR="target/opkg_staging"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/mlua-test"
mkdir -p "$STAGING_DIR/etc/mlua-test"
mkdir -p "$STAGING_DIR/var/lib/mlua-test"
mkdir -p "$STAGING_DIR/etc/init.d"
mkdir -p "$STAGING_DIR/CONTROL"

# Copy binary
BINARY_PATH="target/$TARGET/release/$PKG_NAME"
if [ ! -f "$BINARY_PATH" ]; then
    BINARY_PATH="../target/$TARGET/release/$PKG_NAME"
fi

if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Binary not found at $BINARY_PATH"
    exit 1
fi

cp "$BINARY_PATH" "$STAGING_DIR/usr/bin/"

# Copy Lua libraries
cp lib/*.lua "$STAGING_DIR/usr/share/mlua-test/"

# Copy config template
cp templates/prod_config.lua "$STAGING_DIR/etc/mlua-test/config.lua"

# Create init script
cp services/init.d/mlua-test "$STAGING_DIR/etc/init.d/mlua-test"
chmod +x "$STAGING_DIR/etc/init.d/mlua-test"

# Create control file
cat <<EOF > "$STAGING_DIR/CONTROL/control"
Package: $PKG_NAME
Version: $PKG_VERSION
Architecture: $PKG_ARCH
Maintainer: Antigravity
Section: utils
Priority: optional
Depends: unzip
Description: mlua-test application with Lua 5.5 and SQLite3
Source: https://github.com/kzagar/rust-lua
EOF

# Create conffiles
cat <<EOF > "$STAGING_DIR/CONTROL/conffiles"
/etc/mlua-test/config.lua
/var/lib/mlua-test/server.db
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

(
    cd "$STAGING_DIR/CONTROL"
    tar czf "../../opkg_out/control.tar.gz" .
)

(
    cd "$STAGING_DIR"
    tar czf "../opkg_out/data.tar.gz" --exclude CONTROL .
)

echo "2.0" > "$PKGOUT_DIR/debian-binary"

PKG_FILE="${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.ipk"
(
    cd "$PKGOUT_DIR"
    ar r "../../$PKG_FILE" debian-binary control.tar.gz data.tar.gz
)

echo "### Done! Package is at $PKG_FILE ###"
