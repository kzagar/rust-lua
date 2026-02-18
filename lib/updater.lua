local github = require("github")

local updater = {}

local function normalize_version(v)
    if not v then return "" end
    -- Remove 'v' prefix
    v = v:gsub("^v", "")
    -- Remove git hash (everything after '-')
    v = v:gsub("-.*$", "")
    return v
end

function updater.update()
    local env = MLUA_TEST_ENV
    local current_version = MLUA_TEST_VERSION
    local gh = github.new("kzagar", "rust-lua")

    print("Checking for updates... (Current version: " .. current_version .. ", Env: " .. env .. ")")

    if env == "prod" then
        local release_files, err = gh:get_latest_release()
        if not release_files then return nil, err end
        local latest_version = release_files.version

        if normalize_version(latest_version) == normalize_version(current_version) then
            print("Already up to date (Current: " .. current_version .. ", Latest: " .. latest_version .. ").")
            return false
        end

        print("New version available: " .. latest_version)

        local pkg_file
        for _, f in ipairs(release_files) do
            if f.name:find("%.ipk$") or f.name:find("%.deb$") then
                pkg_file = f
                break
            end
        end

        if not pkg_file then
            return nil, "No package file (ipk/deb) found in release"
        end

        local blob, err = pkg_file:get_blob()
        if not blob then return nil, "Failed to download package: " .. (err or "unknown error") end

        local tmp_path = "/tmp/" .. pkg_file.name
        local f = io.open(tmp_path, "wb")
        f:write(blob)
        f:close()

        print("Installing package " .. tmp_path .. "...")
        local res
        if pkg_file.name:find("%.ipk$") then
            res = util.execute({"opkg", "install", "--force-reinstall", tmp_path})
        else
            res = util.execute({"dpkg", "-i", tmp_path})
        end

        if not res.success then
            return nil, "Failed to install package: " .. (res.stderr or "unknown error")
        end
    else
        -- Staging
        local workflow = gh:workflow("ci.yml")
        local latest_artifact, err = workflow:get_latest()
        if not latest_artifact then return nil, "Failed to get latest artifact: " .. (err or "unknown error") end

        local latest_version = latest_artifact.version
        -- current_version is "0.1.0-hash", latest_version is the full hash
        if current_version:find(latest_version:sub(1, 7)) then
            print("Already up to date.")
            return false
        end

        print("New staging version available: " .. latest_version:sub(1, 7))

        local blob, err = latest_artifact:get_blob()
        if not blob then return nil, "Failed to download artifact: " .. (err or "unknown error") end

        local tmp_dir = "/tmp/mlua-test-staging-update"
        util.execute({"rm", "-rf", tmp_dir})
        util.execute({"mkdir", "-p", tmp_dir})

        local zip_path = tmp_dir .. "/update.zip"
        local f = io.open(zip_path, "wb")
        f:write(blob)
        f:close()

        print("Unzipping update...")
        local res = util.execute({"unzip", "-o", zip_path, "-d", tmp_dir})
        if not res.success then return nil, "Failed to unzip: " .. (res.stderr or "unknown error") end

        -- Paths for staging
        local bin_dest = "/usr/local/bin/mlua-test-staging"
        local lib_dest = "/usr/share/mlua-test-staging"

        print("Installing staging update...")
        -- We assume artifact contains 'mlua-test' binary and 'lib/' directory
        -- Use rm -f before cp to avoid "Text file busy"
        util.execute({"rm", "-f", bin_dest})
        util.execute({"cp", tmp_dir .. "/mlua-test", bin_dest})
        util.execute({"chmod", "+x", bin_dest})
        util.execute({"mkdir", "-p", lib_dest})
        util.execute({"cp", "-r", tmp_dir .. "/lib/.", lib_dest})
    end

    print("Update installed successfully. Exiting for restart...")
    exit(0)
    return true
end

return updater
