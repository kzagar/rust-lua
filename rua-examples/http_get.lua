-- Example of making an HTTPS request using rua-resty-http
-- Note: rua currently has limited support for control flow and table literals.

local httpc = resty.http.new()
local res = httpc:request_uri("https://httpbin.org/get", nil)

print("HTTPS Request to httpbin.org")
print("Status:")
print(res.status)
print("Body Length:")
print(#res.body)
