local telekom_5g = {}

local Telekom5G = {}
Telekom5G.__index = Telekom5G

-- Defaults
local DEFAULT_HOST = "http://192.168.0.1"
local LOGIN_PATH = "/web/v1/user/login"
local PF_PATH = "/web/v1/setting/firewall/portforwarding"

function telekom_5g.new(host, username, password)
    local self = setmetatable({}, Telekom5G)
    
    self.host = host or os.getenv("TELEKOM_5G_URL") or DEFAULT_HOST
    -- Ensure no trailing slash
    if self.host:sub(-1) == "/" then
        self.host = self.host:sub(1, -2)
    end
    
    self.username = username or os.getenv("TELEKOM_5G_USER") or "admin"
    -- Password has no default; must be provided or in env
    self.password = password or os.getenv("TELEKOM_5G_PASS")
    
    self.httpc = http.new()
    self.token = nil
    self.port_forwarding = {}
    
    return self
end

function Telekom5G:login()
    local url = self.host .. LOGIN_PATH
    local payload = {
        username = self.username,
        password = self.password
    }
    
    -- The JSON encoding should handle nil password gracefully (or error out if library strict), 
    -- but usually we want to send what we have.
    
    local res, err = self.httpc:request_uri(url, {
        method = "POST",
        body = json.encode(payload),
        headers = {
            ["Content-Type"] = "application/json"
        }
    })
    
    if not res then
        return nil, "Failed to connect to router: " .. tostring(err)
    end
    
    if res.status ~= 200 then
        return nil, "Login failed with status: " .. tostring(res.status)
    end
    
    local data = json.decode(res.body)
    if not data then
        return nil, "Failed to decode login response"
    end
    
    -- "Authorization" token logic
    local token = nil
    -- Based on README: data.Authorization
    if data.data and data.data.Authorization then
        token = data.data.Authorization
    elseif data.Authorization then
        token = data.Authorization
    end
    
    if not token then
        return nil, "Token not found in login response"
    end
    
    self.token = token
    
    -- "When logging in, it reads all port forwardings"
    return self:refresh_port_forwarding()
end

function Telekom5G:refresh_port_forwarding()
    if not self.token then
        return nil, "Not logged in"
    end
    
    local url = self.host .. PF_PATH
    local res, err = self.httpc:request_uri(url, {
        method = "GET",
        headers = {
            ["Authorization"] = self.token
        }
    })
    
    if not res then
        return nil, "Failed to fetch port forwardings: " .. tostring(err)
    end
    
    if res.status ~= 200 then
        return nil, "Fetch port forwardings failed with status: " .. tostring(res.status)
    end
    
    local data = json.decode(res.body)
    if not data then
        return nil, "Failed to decode port forwarding response"
    end
    
    if data.data.PortForwardings then
        self.port_forwarding = data.data.PortForwardings
    else
        self.port_forwarding = {}
    end
    
    return self.port_forwarding
end

function Telekom5G:add_port_forwarding(rule)
    if not self.token then
        return nil, "Not logged in"
    end
    
    local new_rule = {
        Application = rule.Application,
        PortFrom = tostring(rule.PortFrom),
        Protocol = rule.Protocol,
        IpAddress = rule.IpAddress,
        PortTo = tostring(rule.PortTo),
        Enable = (rule.Enable == nil and true) or rule.Enable,
        IndexId = "",
        OperateType = "insert"
    }
    
    local payload = {
        PortForwardings = { new_rule }
    }
    
    local url = self.host .. PF_PATH
    local res, err = self.httpc:request_uri(url, {
        method = "POST",
        body = json.encode(payload),
        headers = {
            ["Authorization"] = self.token,
            ["Content-Type"] = "application/json"
        }
    })
    
    if not res then
        return nil, "Failed to add rule: " .. tostring(err)
    end
    
    if res.status ~= 200 then
        return nil, "Add rule failed with status: " .. tostring(res.status)
    end
    
    return self:refresh_port_forwarding()
end

function Telekom5G:delete_port_forwarding(index_id)
    if not self.token then
        return nil, "Not logged in"
    end
    
    local payload = {
        PortForwardings = {
            {
                IndexId = tostring(index_id),
                OperateType = "delete"
            }
        }
    }
    
    local url = self.host .. PF_PATH
    
    local res, err = self.httpc:request_uri(url, {
        method = "DELETE",
        body = json.encode(payload),
        headers = {
            ["Authorization"] = self.token,
            ["Content-Type"] = "application/json"
        }
    })
    
    if not res then
        return nil, "Failed to delete rule: " .. tostring(err)
    end
    
    if res.status ~= 200 then
        return nil, "Delete rule failed with status: " .. tostring(res.status)
    end
    
    return self:refresh_port_forwarding()
end

return telekom_5g
