local git = {}

local repo_methods = {}
repo_methods.__index = repo_methods

function repo_methods:pull()
    local res = util.execute({"git", "pull"}, {cwd = self.path})
    if res.success then
        return true, res.stdout
    else
        return false, res.stderr
    end
end

function git.new(path)
    -- Check if directory exists
    -- We can use git rev-parse to check if it's a repo
    local res = util.execute({"git", "-C", path, "rev-parse", "--is-inside-work-tree"})
    if not res.success then
        error("Not a git repository or path does not exist: " .. path)
    end

    local obj = {
        path = path
    }
    setmetatable(obj, repo_methods)
    return obj
end

return git
