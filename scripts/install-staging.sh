#!/bin/bash
set -e

REPO="kzagar/rust-lua"
BIN_DEST="/usr/local/bin/lumen-staging"
CONFIG_DIR="/etc/lumen-staging"
DATA_DIR="/var/lib/lumen-staging"
LIB_DIR="/usr/share/lumen-staging"

if [ -z "$GITHUB_TOKEN" ]; then
    echo "Error: GITHUB_TOKEN environment variable is required."
    exit 1
fi

# Check for dependencies
for cmd in curl jq unzip; do
    if ! command -v "$cmd" &> /dev/null; then
        echo "Error: $cmd is not installed."
        exit 1
    fi
done

echo "### Installing Lumen Staging ###"

# 1. Get latest artifact
echo "Fetching latest artifact info from GitHub..."
# Note: we use ci.yml which we will create in the next step
RUN_ID=$(curl -s -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/$REPO/actions/workflows/ci.yml/runs?status=success&per_page=1" \
  | jq -r '.workflow_runs[0].id')

if [ "$RUN_ID" == "null" ] || [ -z "$RUN_ID" ]; then
    echo "Error: No successful CI runs found for ci.yml."
    exit 1
fi

ARTIFACT_URL=$(curl -s -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/$REPO/actions/runs/$RUN_ID/artifacts" \
  | jq -r '.artifacts[0].archive_download_url')

if [ "$ARTIFACT_URL" == "null" ] || [ -z "$ARTIFACT_URL" ]; then
    echo "Error: No artifacts found for run $RUN_ID."
    exit 1
fi

# 2. Download and unzip
TMP_DIR=$(mktemp -d)
echo "Downloading artifact to $TMP_DIR..."
curl -L -H "Authorization: token $GITHUB_TOKEN" -o "$TMP_DIR/artifact.zip" "$ARTIFACT_URL"
unzip -o "$TMP_DIR/artifact.zip" -d "$TMP_DIR"

# 3. Install binary and libs
echo "Installing files..."
sudo mkdir -p "$(dirname "$BIN_DEST")"
sudo cp "$TMP_DIR/lumen" "$BIN_DEST"
sudo chmod +x "$BIN_DEST"

sudo mkdir -p "$LIB_DIR"
if [ -d "$TMP_DIR/lib" ]; then
    sudo cp -r "$TMP_DIR/lib/." "$LIB_DIR/"
fi

# 4. Setup config and data dirs
sudo mkdir -p "$CONFIG_DIR"
sudo mkdir -p "$DATA_DIR"

# Copy template if it doesn't exist
if [ ! -f "$CONFIG_DIR/config.lua" ]; then
    echo "Creating default config.lua from template..."
    # We assume the script is run from the repo root or templates are reachable
    if [ -f "templates/staging_config.lua" ]; then
        sudo cp templates/staging_config.lua "$CONFIG_DIR/config.lua"
    else
        cat <<EOF | sudo tee "$CONFIG_DIR/config.lua" > /dev/null
-- Lumen Staging Config
util.load_secrets(".secrets")
package.path = package.path .. ";$LIB_DIR/?.lua"
DATA_DIR = "$DATA_DIR"
CONFIG_DIR = "$CONFIG_DIR"
print("Lumen staging started")
EOF
    fi
fi

# 5. Install systemd service
if [ -d "/etc/systemd/system" ]; then
    echo "Installing systemd service..."
    if [ -f "services/systemd/lumen-staging.service" ]; then
        sudo cp services/systemd/lumen-staging.service /etc/systemd/system/
    else
        cat <<EOF | sudo tee /etc/systemd/system/lumen-staging.service > /dev/null
[Unit]
Description=Lumen Staging
After=network.target

[Service]
ExecStart=$BIN_DEST $CONFIG_DIR/config.lua
WorkingDirectory=$DATA_DIR
Restart=always
RestartSec=5
StandardOutput=syslog
StandardError=syslog
SyslogIdentifier=lumen-staging

[Install]
WantedBy=multi-user.target
EOF
    fi
    sudo systemctl daemon-reload
    sudo systemctl enable lumen-staging.service
    echo "Service lumen-staging.service installed and enabled."
fi

# 6. Install init.d service
if [ -d "/etc/init.d" ]; then
    echo "Installing init.d service..."
    if [ -f "services/init.d/lumen-staging" ]; then
        sudo cp services/init.d/lumen-staging /etc/init.d/
    else
        cat <<EOF | sudo tee /etc/init.d/lumen-staging > /dev/null
#!/bin/sh /etc/rc.common

START=99
USE_PROCD=1

start_service() {
    procd_open_instance
    procd_set_param command $BIN_DEST $CONFIG_DIR/config.lua
    procd_set_param stdout 1
    procd_set_param stderr 1
    procd_set_param pwd $DATA_DIR
    procd_set_param respawn
    procd_close_instance
}
EOF
    fi
    sudo chmod +x /etc/init.d/lumen-staging
    if command -v /etc/init.d/lumen-staging &> /dev/null; then
        sudo /etc/init.d/lumen-staging enable
    fi
    echo "Service lumen-staging installed and enabled."
fi

rm -rf "$TMP_DIR"
echo "### Installation complete! ###"
