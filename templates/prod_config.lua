-- mlua-test Production Config
util.load_secrets(".secrets")

-- Set Lua library path
package.path = package.path .. ";/usr/share/mlua-test/?.lua"

-- Global paths for scripts
DATA_DIR = "/var/lib/mlua-test"
CONFIG_DIR = "/etc/mlua-test"

print("mlua-test prod started (Version: " .. MLUA_TEST_VERSION .. ")")

-- Load application scripts
-- require("main_app")
