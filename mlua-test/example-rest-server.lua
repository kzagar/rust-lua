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

-- Background cron job
local scheduler = cron.new()
-- Run every 10 seconds
scheduler:register("0/10 * * * * *", function()
    print("--- [Cron] Adding automated user to database ---")
    add_user_to_db({ name = "Bot_" .. uuid():sub(1,4), role = "bot" })
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

-- Simple hello endpoint
srv:register("/api/hello", "GET", function(params)
    local name = params.name or "World"
    return {
        message = "Hello, " .. name .. "!",
        timestamp = os.time(),
        params = params
    }
end)

-- Serve static files
srv:serve_static("/", "public")

print("Server configured with Database ORM, Cron, and Static files.")
