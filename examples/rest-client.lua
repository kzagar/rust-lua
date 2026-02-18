local httpc = http.new()

print("Fetching httpbin.org/get...")
local res, err = httpc:request_uri("https://httpbin.org/get", {
    method = "GET",
    headers = {
        ["Accept"] = "application/json",
    }
})

if not res then
    print("Error: " .. tostring(err))
    return
end

print("Status: " .. res.status)
print("Headers:")
for k, v in pairs(res.headers) do
    print("  " .. k .. ": " .. v)
end

print("\nBody:")
print(res.body)

print("\nDecoding JSON body...")
local data = json.decode(res.body)
if data then
    print("Successfully decoded JSON!")
    print("Origin: " .. (data.origin or "unknown"))
    print("URL: " .. (data.url or "unknown"))
    
    -- Print some headers from the JSON response
    if data.headers then
        print("Headers from response JSON:")
        for k, v in pairs(data.headers) do
            print("  " .. k .. ": " .. v)
        end
    end
else
    print("Failed to decode JSON")
end

print("\nTesting JSON encoding...")
local test_table = {
    name = "Lumen",
    features = {"http", "json", "sqlite3"},
    active = true,
    version = 0.1
}
local encoded = json.encode(test_table)
print("Encoded JSON: " .. encoded)
