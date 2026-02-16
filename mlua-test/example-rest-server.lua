local srv = rest.new()

-- Register a GET endpoint at /api/users
-- The callback receives a table of query parameters
-- and returns a table (list of dictionaries) that will be converted to JSON
srv:register("/api/users", "GET", function(params)
    print("--- [Lua] Handling /api/users request ---")
    print("Query parameters: " .. json.encode(params))

    local filter_role = params.role
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

-- Register another endpoint
srv:register("/api/hello", "GET", function(params)
    local name = params.name or "World"
    return {
        message = "Hello, " .. name .. "!",
        timestamp = os.time(),
        params = params
    }
end)

-- To use plain HTTP:
-- srv:listen("0.0.0.0:3000")

print("Starting REST server (TLS) on https://0.0.0.0:3443")
print("Try calling: curl -k \"https://localhost:3443/api/users?role=user\"")
srv:listen_tls("0.0.0.0:3443", "cert.pem", "key.pem")
