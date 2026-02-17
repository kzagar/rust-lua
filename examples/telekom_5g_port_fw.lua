local telekom = require("telekom_5g")

-- Configuration default to environment variables, or hardcoded fallback for safety (empty/nil will fail login)
local router_url = os.getenv("TELEKOM_5G_URL") or "http://127.0.0.1:9999"
local user = os.getenv("TELEKOM_5G_USER")
local pass = os.getenv("TELEKOM_5G_PASS")

if not user or not pass then
    print("Error: Please set TELEKOM_5G_USER and TELEKOM_5G_PASS environment variables.")
    os.exit(1)
end

local t5g = telekom.new(router_url, user, pass)

print(string.format("Logging into %s as %s...", router_url, user))
local pf_list, err = t5g:login()
if not pf_list then
    print("Login failed: " .. tostring(err))
    os.exit(1)
end

local function print_rules(rules, label)
    print(string.format("\n--- %s (%d rules) ---", label, #rules))
    if #rules == 0 then
        print("  (No rules)")
    else
        for i, rule in ipairs(rules) do
            print(string.format("  [%s] %s: %s:%s -> %s:%s (%s)", 
                rule.IndexId or "?",
                rule.Application,
                rule.Protocol,
                rule.PortFrom,
                rule.IpAddress,
                rule.PortTo,
                tostring(rule.Enable)
            ))
        end
    end
    print("----------------------------")
end

-- 1. Print all port forwardings
print_rules(pf_list, "Initial Port Forwardings")

-- Define a test rule
local test_app_name = "LuaTestRule"
local test_rule = {
    Application = test_app_name,
    PortFrom = "22222",
    Protocol = "TCP",
    IpAddress = "192.168.0.123",
    PortTo = "22",
    Enable = true
}

-- 2. Add one
print(string.format("\nAdding rule: %s (%s -> %s:%s)...", test_rule.Application, test_rule.PortFrom, test_rule.IpAddress, test_rule.PortTo))
pf_list, err = t5g:add_port_forwarding(test_rule)

if not pf_list then
    print("Failed to add rule: " .. tostring(err))
    os.exit(1)
end

-- 3. Print list
print_rules(pf_list, "After Adding Rule")

-- Find the rule we just added to get its IndexId
local added_rule_id = nil
for _, rule in ipairs(pf_list) do
    if rule.Application == test_app_name and rule.PortFrom == test_rule.PortFrom then
        added_rule_id = rule.IndexId
        break
    end
end

if not added_rule_id then
    print("Error: Could not find the added rule (IndexId) in the refreshed list!")
    os.exit(1)
end

print("Found added rule ID: " .. tostring(added_rule_id))

-- 4. Delete one
print("\nDeleting rule ID: " .. tostring(added_rule_id) .. "...")
pf_list, err = t5g:delete_port_forwarding(added_rule_id)

if not pf_list then
    print("Failed to delete rule: " .. tostring(err))
    os.exit(1)
end

-- 5. Print list
print_rules(pf_list, "After Deleting Rule")

print("\nTest completed successfully.")
