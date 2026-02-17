local ourgroceries = require("ourgroceries")

local user = os.getenv("OURGROCERIES_USER")
local pass = os.getenv("OURGROCERIES_PASS")

if not user or not pass then
    print("Please set OURGROCERIES_USER and OURGROCERIES_PASS environment variables")
    -- For demonstration, we'll stop here.
    return
end

local og, err = ourgroceries.new(user, pass)
if not og then
    print("Failed to login: " .. tostring(err))
    return
end

print("Logged in successfully!")

local lists, err = og:get_lists()
if not lists then
    print("Failed to get lists: " .. tostring(err))
    return
end

for _, list in ipairs(lists) do
    print(string.format("List: %s (id: %s)", list.name, list.id))
    local items, err = list:get_items()
    if items then
        for _, item in ipairs(items) do
            local status = item.crossedOff and "[X]" or "[ ]"
            print(string.format("  %s %s", status, item.name))
        end
    else
        print("  Failed to get items: " .. tostring(err))
    end
end
