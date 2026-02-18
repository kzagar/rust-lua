-- mlua-test Staging Config
util.load_secrets(".secrets")

-- Set Lua library path
package.path = package.path .. ";/usr/share/mlua-test-staging/?.lua"

-- Global paths for scripts
DATA_DIR = "/var/lib/mlua-test-staging"
CONFIG_DIR = "/etc/mlua-test-staging"

local lkg_binary = DATA_DIR .. "/mlua-test.lkg"
local current_binary = "/usr/local/bin/mlua-test-staging"

-- Sanity check
local function sanity_check()
    -- Add your sanity checks here
    -- For example, check if we can load a core library
    -- local ok = pcall(require, "updater")
    -- if not ok then return false, "Cannot load updater library" end
    return true
end

print("mlua-test staging starting (Version: " .. MLUA_TEST_VERSION .. ")")

local ok, err = sanity_check()
if not ok then
    print("Sanity check failed: " .. err)
    local lkg_exists = util.execute({"test", "-f", lkg_binary}).success
    if lkg_exists then
        print("Rolling back to Last Known Good version...")
        util.execute({"rm", "-f", current_binary})
        util.execute({"cp", lkg_binary, current_binary})
        -- Give some time to avoid tight restart loop if rollback also fails
        wait(5)
        exit(1)
    else
        print("No LKG version found. Continuing anyway...")
    end
else
    print("Sanity check passed. Marking current version as LKG.")
    util.execute({"rm", "-f", lkg_binary})
    util.execute({"cp", current_binary, lkg_binary})
end

-- Load application scripts
-- require("main_app")
