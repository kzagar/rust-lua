-- Reverse Proxy Example with Authorization

-- 1. Static Authorization Implementation
-- This uses a Lua table to define authorized users for a domain.
local static_authorized_users = {
    ["example.com"] = {
        ["admin@gmail.com"] = true,
        ["user@gmail.com"] = true,
    }
}

function is_authorized_static(email, domain)
    print("Checking static auth for " .. email .. " in " .. domain)
    local domain_users = static_authorized_users[domain]
    if domain_users and domain_users[email] then
        return true
    end
    return false
end

-- 2. SQLite Authorization Implementation
-- This uses the built-in sqlite3 module to check authorization.
async function is_authorized_sqlite(email, domain)
    print("Checking SQLite auth for " .. email .. " in " .. domain)
    local db = await sqlite3.open("server.db")
    local count = await db.count("authorized_users", { domain = domain, email = email })
    await db.close()
    return count > 0
end

-- Setup Domain via API (manages authorized_users table in server.db)
reverse_proxy.domain("corp.internal").add_user("boss@gmail.com")

-- Configure Proxies

-- Public proxy (no auth)
reverse_proxy.add("localhost", "/public", "https://httpbin.org")

-- Proxy with static auth
reverse_proxy.add("localhost", "/static-secret", "https://httpbin.org")
    :require_auth("example.com")
    :auth_callback(is_authorized_static)

-- Proxy with SQLite auth (using the Lua implementation)
reverse_proxy.add("localhost", "/sqlite-secret", "https://httpbin.org")
    :require_auth("corp.internal")
    :auth_callback(is_authorized_sqlite)

-- Proxy with BUILT-IN Rust SQLite auth (default behavior if no auth_callback)
reverse_proxy.add("localhost", "/built-in-secret", "https://httpbin.org")
    :require_auth("corp.internal")

print("Reverse proxy example loaded. Listening on port 3443 (default HTTPS).")
print("Authorized for example.com: admin@gmail.com, user@gmail.com")
print("Authorized for corp.internal: boss@gmail.com")
