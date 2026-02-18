local github = {}

local gh_methods = {}
gh_methods.__index = gh_methods

local workflow_methods = {}
workflow_methods.__index = workflow_methods

local httpc = http.new()
local token = os.getenv("GITHUB_TOKEN")

local function request(method, path, params, accept_header)
    local headers = {
        ["Accept"] = accept_header or "application/vnd.github+json",
        ["X-GitHub-Api-Version"] = "2022-11-28",
        ["User-Agent"] = "Lumen"
    }
    if token then
        headers["Authorization"] = "Bearer " .. token
    end

    local url_full = "https://api.github.com" .. path
    local body = nil
    if method ~= "GET" and params then
        body = json.encode(params)
        headers["Content-Type"] = "application/json"
    elseif method == "GET" and params then
        url_full = url_full .. "?" .. url.encode_query(params)
    end

    local res, err = httpc:request_uri(url_full, {
        method = method,
        headers = headers,
        body = body
    })

    if not res then
        return nil, err
    end

    if res.status >= 200 and res.status < 300 then
        local ct = res.headers["content-type"] or ""
        if ct:find("application/json") and not accept_header then
            return json.decode(res.body), nil
        else
            return res.body, nil
        end
    else
        return nil, "GitHub API error: " .. res.status .. " " .. (res.body or "")
    end
end

function workflow_methods:get_latest()
    -- Get latest successful run
    local path = string.format("/repos/%s/%s/actions/workflows/%s/runs", self.owner, self.repo, self.id)
    local data, err = request("GET", path, { status = "success", per_page = 1 })
    if not data then return nil, err end

    if not data.workflow_runs or #data.workflow_runs == 0 then
        return nil, "No successful runs found"
    end

    local run = data.workflow_runs[1]

    -- Get artifacts for this run
    local artifacts_path = string.format("/repos/%s/%s/actions/runs/%s/artifacts", self.owner, self.repo, run.id)
    local art_data, err = request("GET", artifacts_path)
    if not art_data then return nil, err end

    if not art_data.artifacts or #art_data.artifacts == 0 then
        return nil, "No artifacts found for the latest successful run"
    end

    local artifact = art_data.artifacts[1]

    local f = file.new(artifact.name .. ".zip")
    f = f:mime("application/zip")
    f.id = tostring(artifact.id)
    f.version = run.head_sha -- Store the commit SHA as version

    local owner, repo = self.owner, self.repo
    f:set_downloader(function(file_obj)
        local download_path = string.format("/repos/%s/%s/actions/artifacts/%s/zip", owner, repo, file_obj.id)
        local data, err = request("GET", download_path)
        if not data then error(err) end
        return data
    end)

    return f
end

function gh_methods:workflow(id_or_name)
    local obj = {
        owner = self.owner,
        repo = self.repo,
        id = id_or_name
    }
    setmetatable(obj, workflow_methods)
    return obj
end

function gh_methods:get_latest_release()
    local path = string.format("/repos/%s/%s/releases/latest", self.owner, self.repo)
    local data, err = request("GET", path)
    if not data then return nil, err end

    local files = { version = data.tag_name }
    for _, asset in ipairs(data.assets or {}) do
        local f = file.new(asset.name)
        f = f:mime(asset.content_type)
        f.id = tostring(asset.id)
        f.version = data.tag_name

        local owner, repo = self.owner, self.repo
        f:set_downloader(function(file_obj)
            local download_path = string.format("/repos/%s/%s/releases/assets/%s", owner, repo, file_obj.id)
            local data, err = request("GET", download_path, nil, "application/octet-stream")
            if not data then error(err) end
            return data
        end)
        table.insert(files, f)
    end

    return files
end

function github.new(owner, repo)
    local obj = {
        owner = owner,
        repo = repo
    }
    setmetatable(obj, gh_methods)
    return obj
end

return github
