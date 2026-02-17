local ourgroceries = {}

local SIGN_IN_URL = "https://www.ourgroceries.com/sign-in"
local YOUR_LISTS_URL = "https://www.ourgroceries.com/your-lists/"

local OurGroceries = {}
OurGroceries.__index = OurGroceries

local List = {}
List.__index = List

function ourgroceries.new(username, password)
    local self = setmetatable({}, OurGroceries)
    self.username = username or os.getenv("OURGROCERIES_USER")
    self.password = password or os.getenv("OURGROCERIES_PASS")
    self.httpc = http.new({ user_agent = "Mozilla/5.0" })
    self.team_id = nil

    local ok, err = self:login()
    if not ok then
        return nil, err
    end

    return self
end

function OurGroceries:login()
    logging.debug("Logging in to OurGroceries")
    local params = {
        emailAddress = self.username,
        password = self.password,
        action = "sign-in"
    }

    local res, err = self.httpc:request_uri(SIGN_IN_URL, {
        method = "POST",
        body = url.encode_query(params),
        headers = {
            ["Content-Type"] = "application/x-www-form-urlencoded"
        }
    })

    if not res then
        return nil, "Failed to send login request: " .. tostring(err)
    end

    if res.status < 200 or res.status >= 300 then
        return nil, "Login failed with status: " .. tostring(res.status)
    end

    return self:refresh_team_id()
end

function OurGroceries:refresh_team_id()
    logging.debug("Refreshing team ID")
    local res, err = self.httpc:request_uri(YOUR_LISTS_URL, {
        method = "GET"
    })

    if not res then
        return nil, "Failed to get your-lists page: " .. tostring(err)
    end

    local re_team_id = re.compile([[g_teamId = "(.*)";]])
    local caps = re_team_id:match(res.body)

    if caps and caps[1] then
        self.team_id = caps[1]
        logging.debug("Found team ID: " .. self.team_id)
        return true
    else
        return nil, "Could not find team ID in response"
    end
end

function OurGroceries:post_command(command, payload)
    if not self.team_id then
        local ok, err = self:login()
        if not ok then
            return nil, "Team ID still missing after login: " .. tostring(err)
        end
    end

    payload.command = command
    payload.teamId = self.team_id

    local res, err = self.httpc:request_uri(YOUR_LISTS_URL, {
        method = "POST",
        body = json.encode(payload),
        headers = {
            ["Content-Type"] = "application/json"
        }
    })

    if not res then
        return nil, "Failed to send command " .. command .. ": " .. tostring(err)
    end

    if res.status < 200 or res.status >= 300 then
        return nil, "Command " .. command .. " failed with status: " .. tostring(res.status)
    end

    return json.decode(res.body)
end

function OurGroceries:get_lists()
    logging.debug("Getting shopping lists")
    local res, err = self:post_command("getOverview", {})
    if not res then return nil, err end

    local raw_lists = res.shoppingLists
    if not raw_lists then
        return nil, "Missing shoppingLists in response"
    end

    local lists = {}
    for _, l in ipairs(raw_lists) do
        table.insert(lists, List.new(self, l))
    end
    return lists
end

function OurGroceries:get_list_items(list_id)
    logging.debug("Getting items for list: " .. list_id)
    local res, err = self:post_command("getList", { listId = list_id })
    if not res then return nil, err end

    if not res.list or not res.list.items then
        return nil, "Missing list items in response"
    end

    local items = res.list.items
    for _, item in ipairs(items) do
        if item.crossedOffAt then
            item.crossedOff = true
        end
    end
    return items
end

function OurGroceries:delete_all_crossed_off_from_list(list_id)
    logging.debug("Deleting crossed off items from list: " .. list_id)
    local _, err = self:post_command("deleteAllCrossedOffItems", { listId = list_id })
    if err then
        return nil, err
    end
    return true
end

-- List methods
function List.new(og, data)
    local self = setmetatable(data, List)
    self.og = og
    return self
end

function List:get_items()
    return self.og:get_list_items(self.id)
end

function List:delete_crossed_off_items()
    return self.og:delete_all_crossed_off_from_list(self.id)
end

return ourgroceries
