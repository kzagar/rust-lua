-- Lumen Production Config
util.load_secrets(".secrets")

-- Set Lua library path
package.path = package.path .. ";/usr/share/lumen/?.lua"

-- Global paths for scripts
DATA_DIR = "/var/lib/lumen"
CONFIG_DIR = "/etc/lumen"

print("Lumen prod started (Version: " .. LUMEN_VERSION .. ")")

-- Load application scripts
-- require("main_app")
