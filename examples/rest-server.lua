local srv = rest.new()

-- Open database for the server
local db = sqlite3.open("server.db")

-- Initialize database schema
db:exec([[
    CREATE TABLE IF NOT EXISTS users (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        role TEXT NOT NULL DEFAULT 'user'
    )
]])

db:exec([[
    CREATE TABLE IF NOT EXISTS latency_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp REAL NOT NULL,
        latency_ms REAL NOT NULL,
        status_code INTEGER NOT NULL,
        url TEXT NOT NULL
    )
]])

-- Register a GET endpoint at /api/users
-- Uses the high-level ORM 'objects' method
srv:register("/api/users", "GET", function(params)
    print("--- [Lua] Handling /api/users GET request ---")
    
    local filter = {}
    if params.role then
        filter.role = params.role
    end
    if params.name then
        filter.name = like("%" .. params.name .. "%")
    end

    local results = db:objects("users", filter)
    return results
end)

-- GET API to retrieve latency logs for the last X seconds
srv:register("/api/latency", "GET", function(params)
    local seconds = tonumber(params.seconds) or 60
    local cutoff = now() - seconds
    
    print(string.format("--- [Lua] Fetching latency logs since %.2f ---", cutoff))
    
    -- Using the high-level ORM with a comparison operator
    local results = db:objects("latency_logs", {
        timestamp = gt(cutoff)
    })
    
    return results
end)

-- Function to add a user (used by REST and Cron)
local function add_user_to_db(params)
    print("--- [Lua] Adding user to DB ---")
    
    local name = params.name
    if not name then
        return { error = "Missing name parameter" }
    end
    
    local obj = new_object("users", {
        name = name,
        role = params.role or "user"
    })
    
    db:add(obj)
    
    -- Fetch back to get the ID (simple way for now)
    local results = db:objects("users", { name = name })
    return results[#results] or { success = true }
end

-- Register a POST endpoint at /api/users
srv:register("/api/users", "POST", add_user_to_db)

-- HTTP client for the cron job
local http_client = http.new({ insecure = true })

-- Background cron job
local scheduler = cron.new()
-- Run every 10 seconds
scheduler:register("0/10 * * * * *", function()
    print("--- [Cron] Adding automated user to database ---")
    add_user_to_db({ name = "Bot_" .. uuid():sub(1,4), role = "bot" })
end)

-- List of servers to probe in round-robin fashion
local servers = {
    "https://127.0.0.1:3443/api/hello",
    "https://httpbin.org/get",
    "https://google.com",
    "https://github.com",
    "https://wikipedia.org"
}
local next_server_idx = 1

-- Run every 15 seconds (HTTP Latency Probe)
scheduler:register("0/15 * * * * *", function()
    local url = servers[next_server_idx]
    next_server_idx = (next_server_idx % #servers) + 1
    
    print(string.format("--- [Cron] Probing %s ---", url))
    local start_time = now()
    
    local res, err = http_client:request_uri(url)
    local end_time = now()
    
    if res then
        local latency = (end_time - start_time) * 1000
        print(string.format("Response: %d, Latency: %.2f ms", res.status, latency))
        
        local log_entry = new_object("latency_logs", {
            timestamp = start_time,
            latency_ms = latency,
            status_code = res.status,
            url = url
        })
        db:add(log_entry)
    else
        print("Probe failed for " .. url .. ": " .. (err or "unknown error"))
    end
end)

-- Async wait endpoint
srv:register("/api/wait", "GET", function(params)
    local seconds = tonumber(params.seconds) or 1.0
    print("Waiting for " .. seconds .. " seconds...")
    wait(seconds)
    return {
        message = "Waited for " .. seconds .. " seconds",
        seconds = seconds
    }
end)

-- Stats endpoint using db:count
srv:register("/api/stats", "GET", function()
    local total = db:count("users")
    local bots = db:count("users", { role = "bot" })
    local admins = db:count("users", { role = "admin" })
    
    return {
        total_users = total,
        bots = bots,
        admins = admins,
        storage = "sqlite3 (server.db)"
    }
end)

-- Simple hello endpoint
srv:register("/api/hello", "GET", function(params)
    local name = params.name or "World"
    local count = db:count("users")
    return {
        message = "Hello, " .. name .. "!",
        total_registered_users = count,
        timestamp = os.time(),
    }
end)

-- Serve static files
srv:serve_static("/", "public")

print("Server configured with Database ORM, Cron, and Static files.")
