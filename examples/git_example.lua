local git = require("git")

-- Use current directory as it is a git repo
local repo_path = "."
local repo = git.new(repo_path)

print("Attempting to pull " .. repo_path .. "...")
local ok, output = repo:pull()

if ok then
    print("Pull successful:")
    print(output)
else
    print("Pull failed (as expected if no remote or no changes):")
    print(output)
end
