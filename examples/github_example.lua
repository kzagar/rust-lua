local github = require("github")

-- Using tokio as an example
-- Using GitHub CLI as an example (it has many assets)
local owner = "cli"
local repo_name = "cli"

local gh = github.new(owner, repo_name)

print("--- GitHub Release Example ---")
local assets, err = gh:get_latest_release()
if assets then
    print("Latest release for " .. owner .. "/" .. repo_name .. " has " .. #assets .. " assets:")
    for i, asset in ipairs(assets) do
        print(i .. ". " .. asset.name .. " (ID: " .. asset.id .. ", MIME: " .. (asset.mime_type or "unknown") .. ")")
    end

    if #assets > 0 then
        print("\nTesting download of first asset (metadata only in this example)...")
        -- In a real scenario, you would call asset:get_blob()
        -- local data = assets[1]:get_blob()
        -- print("Downloaded " .. #data .. " bytes")
    end
else
    print("Error getting release: " .. (err or "unknown"))
end

print("\n--- GitHub Workflow Artifact Example ---")
local wtf = gh:workflow("CI") -- Use "CI" or a valid workflow ID
local artifact, err = wtf:get_latest()

if artifact then
    print("Found latest successful artifact: " .. artifact.name)
    print("Artifact ID: " .. artifact.id)
    -- local data = artifact:get_blob()
else
    print("Artifact not found or error: " .. (err or "unknown"))
    print("Note: GitHub Actions artifacts require authentication (GITHUB_TOKEN) to download.")
end
