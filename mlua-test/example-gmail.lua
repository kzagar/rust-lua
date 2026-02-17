print("GMail Example starting...")

local email = "your-email@gmail.com" -- Change this to test
local login_res = gmail.login(email)

if login_res.status == "unauthorized" then
    print("Please authorize the application by visiting this URL:")
    print(login_res.auth_url)
    print("Once authorized, run this script again.")
    return
end

local mailbox = login_res.mailbox
print("Authorized as " .. email)

-- Search for messages in the last 24 hours
local now_sec = now()
local yesterday = now_sec - (24 * 60 * 60)
print("Searching for messages after timestamp: " .. yesterday)

local msgs = mailbox:search({ after = math.floor(yesterday) })
print("Found " .. #msgs .. " messages.")

if #msgs > 0 then
    local msg_id = msgs[1]
    print("Retrieving message: " .. msg_id)
    local msg = mailbox:get_message(msg_id)
    local info = msg:get_info()

    print("Subject: " .. (info.headers.Subject or "(No Subject)"))
    print("From: " .. (info.headers.From or "(Unknown)"))
    print("Snippet: " .. (info.snippet or "")) -- I should add snippet to get_info

    local att_paths = msg:download_attachments()
    print("Downloaded " .. #att_paths .. " attachments.")
    for i, path in ipairs(att_paths) do
        print("  - " .. path)
    end
end

-- Example of sending a draft
--[[
local draft_id = mailbox:prepare_draft({
    to = "recipient@example.com",
    subject = "Test from Lua",
    body = "Hello from mlua-test!",
    attachments = {
        ["test.txt"] = "path/to/test.txt"
    }
})
print("Draft created with ID: " .. draft_id)
-- mailbox:send_draft(draft_id)
--]]
