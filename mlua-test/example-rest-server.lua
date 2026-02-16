local srv = rest.new()

-- Register a GET endpoint at /api/users
-- The callback receives a table of query parameters
-- and returns a table (list of dictionaries) that will be converted to JSON
local users = {
    { id = 1, name = "Alice", role = "admin" },
    { id = 2, name = "Bob", role = "user" },
    { id = 3, name = "Charlie", role = "user" },
    { id = 4, name = "Dave", role = "user" },
    { id = 5, name = "Eve", role = "user" },
    { id = 6, name = "Frank", role = "user" },
    { id = 7, name = "Grace", role = "user" },
    { id = 8, name = "Heidi", role = "user" },
    { id = 9, name = "Ivan", role = "user" },
    { id = 10, name = "Judy", role = "user" },
}

-- Register a GET endpoint at /api/users
-- The callback receives a table of query parameters
-- and returns a table (list of dictionaries) that will be converted to JSON
srv:register("/api/users", "GET", function(params)
    print("--- [Lua] Handling /api/users request ---")
    print("Query parameters: " .. json.encode(params))

    local filter_role = params.role
    local result = {}
    if filter_role then
        for _, user in ipairs(users) do
            if user.role == filter_role then
                table.insert(result, user)
            end
        end
    else
        result = users
    end

    return result
end)

srv:register("/api/users", "POST", function(params)
    print("--- [Lua] Handling POST /api/users request ---")
    print("Params: " .. json.encode(params))
    
    local name = params.name
    if not name then
        return { error = "Missing name parameter" }
    end
    
    local id = #users + 1
    local new_user = {
        id = id,
        name = name,
        role = params.role or "user"
    }
    
    table.insert(users, new_user)
    return new_user
end)

-- Register another endpoint
srv:register("/api/hello", "GET", function(params)
    local name = params.name or "World"
    return {
        message = "Hello, " .. name .. "!",
        timestamp = os.time(),
        params = params
    }
end)

-- Serve static files from "public" directory at "/"
srv:serve_static("/", "public")
