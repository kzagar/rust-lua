local results = {}
local httpc = http.new() -- This object is safe to share across parallel tasks

-- get(url) returns a closure that fetches the URL and stores the result
function get(url)
    return function()
        print("[Task] Starting fetch: " .. url)
        
        -- We use the shared 'httpc' object here. 
        -- In mlua, calling an async function from a coroutine will yield.
        local res, err = httpc:request_uri(url)
        
        if res then
            results[url] = "Status: " .. res.status .. ", Body length: " .. #res.body
            print("[Task] Completed fetch: " .. url)
        else
            results[url] = "Error: " .. (err or "unknown")
            print("[Task] Failed fetch: " .. url .. " Error: " .. tostring(err))
        end
    end
end

-- parallel(...) is now implemented in Rust and available globally
-- It takes any number of functions (closures) and executes them concurrently

-- Demonstration: Individual closures
print("[Parallel] Starting 3 tasks using individual arguments...")
parallel(
    get("https://httpbin.org/get?query=parallel-1"),
    get("https://httpbin.org/get?query=parallel-2"),
    get("https://httpbin.org/get?query=parallel-3")
)

-- Demonstration: Flattening (list of closures)
print("\n[Parallel] Starting tasks using a list (flattening)...")
local task_list = {
    get("https://httpbin.org/get?query=list-1"),
    get("https://httpbin.org/get?query=list-2")
}
parallel(task_list, get("https://httpbin.org/get?query=solo"))

-- Demonstration: Sequential execution
print("\n[Sequential] Executing tasks one by one...")
sequential(
    get("https://httpbin.org/get?query=seq-1"),
    get("https://httpbin.org/get?query=seq-2")
)

print("\n[Parallel] Mixed delays (parallel test)...")
parallel(
    get("https://httpbin.org/delay/1"),
    get("https://httpbin.org/delay/2")
)

print("[Status] All demonstrations completed.")

print("\n" .. string.rep("=", 60))
print(string.format("%-40s | %s", "URL", "Result"))
print(string.rep("-", 60))
for url, result in pairs(results) do
    print(string.format("%-40s | %s", url, result))
end
print(string.rep("=", 60))
