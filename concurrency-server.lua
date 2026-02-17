local srv = rest.new()
srv:listen("0.0.0.0:8080")

srv:register("/wait", "GET", function(params)
    local seconds = tonumber(params.seconds) or 20
    print("Received /wait for " .. seconds .. " seconds")
    wait(seconds)
    return { status = "waited", seconds = seconds }
end)

srv:register("/query", "GET", function()
    -- Small delay to simulate some work if needed,
    -- but for concurrency test we usually want to see VM overhead
    return { status = "ok", timestamp = now() }
end)

print("Concurrency test server configured on http://0.0.0.0:8080")
