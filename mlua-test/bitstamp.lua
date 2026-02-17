local bitstamp = {}

local api_key = os.getenv("BITSTAMP_API_KEY")
local api_secret = os.getenv("BITSTAMP_API_SECRET")
local is_sandbox = os.getenv("BITSTAMP_SANDBOX") == "true"

local base_url = is_sandbox and "https://sandbox.bitstamp.net" or "https://www.bitstamp.net"
local host = is_sandbox and "sandbox.bitstamp.net" or "www.bitstamp.net"

local httpc = http.new()

local function private_request(method, path, params)
    if not api_key or not api_secret then
        return nil, "BITSTAMP_API_KEY and BITSTAMP_API_SECRET must be set"
    end

    local nonce = uuid()
    local timestamp = string.format("%.0f", now() * 1000)
    local query = ""
    local body = ""
    local content_type = nil

    if method == "GET" then
        if params and next(params) then
            query = "?" .. url.encode_query(params)
        end
    else
        if params and next(params) then
            body = url.encode_query(params)
            content_type = "application/x-www-form-urlencoded"
        end
    end

    -- string_to_sign = "BITSTAMP" + " " + api_key + HTTP Verb + url.host + url.path + url.query + Content-Type + X-Auth-Nonce + X-Auth-Timestamp + X-Auth-Version + request.body
    local message = "BITSTAMP " .. api_key ..
                    method ..
                    host ..
                    path ..
                    query ..
                    (content_type or "") ..
                    nonce ..
                    timestamp ..
                    "v2" ..
                    body

    local signature = crypto.hmac_sha256(api_secret, message):upper()

    local headers = {
        ["X-Auth"] = "BITSTAMP " .. api_key,
        ["X-Auth-Signature"] = signature,
        ["X-Auth-Nonce"] = nonce,
        ["X-Auth-Timestamp"] = timestamp,
        ["X-Auth-Version"] = "v2",
    }
    if content_type then
        headers["Content-Type"] = content_type
    end

    local url_full = base_url .. path .. query
    local res, err = httpc:request_uri(url_full, {
        method = method,
        headers = headers,
        body = method ~= "GET" and body ~= "" and body or nil
    })

    if not res then
        return nil, err
    end

    local decoded = json.decode(res.body)
    if res.status >= 200 and res.status < 300 then
        return decoded, nil
    else
        return nil, decoded or res.body
    end
end

local function public_request(path, params)
    local query = ""
    if params and next(params) then
        query = "?" .. url.encode_query(params)
    end
    local url_full = base_url .. path .. query
    local res, err = httpc:request_uri(url_full, {
        method = "GET"
    })
    if not res then
        return nil, err
    end
    local decoded = json.decode(res.body)
    if res.status >= 200 and res.status < 300 then
        return decoded, nil
    else
        return nil, decoded or res.body
    end
end

function bitstamp.get_ticker(pair)
    return public_request("/api/v2/ticker/" .. pair .. "/")
end

function bitstamp.get_open_orders(pair)
    local path = "/api/v2/open_orders/"
    if pair then
        path = path .. pair .. "/"
    end
    return private_request("POST", path)
end

function bitstamp.buy_limit(pair, amount, price)
    return private_request("POST", "/api/v2/buy/" .. pair .. "/", {
        amount = tostring(amount),
        price = tostring(price)
    })
end

function bitstamp.sell_limit(pair, amount, price)
    return private_request("POST", "/api/v2/sell/" .. pair .. "/", {
        amount = tostring(amount),
        price = tostring(price)
    })
end

function bitstamp.buy_market(pair, amount)
    return private_request("POST", "/api/v2/buy/market/" .. pair .. "/", {
        amount = tostring(amount)
    })
end

function bitstamp.sell_market(pair, amount)
    return private_request("POST", "/api/v2/sell/market/" .. pair .. "/", {
        amount = tostring(amount)
    })
end

function bitstamp.cancel_order(order_id)
    return private_request("POST", "/api/v2/cancel_order/", {
        id = tostring(order_id)
    })
end

return bitstamp
