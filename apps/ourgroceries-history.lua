local ourgroceries = require("ourgroceries")

local DB_PATH = os.getenv("GROCERIES_DB_PATH") or "./ourgroceries-history.db"

local function parse_quantity(item_name)
    -- Try to match patterns like "2x Milk", "1.5L Water", "2 Milk"
    -- Pattern: optional number at the start followed by optional units and then the name
    local q, n = item_name:match("^([%d%.]+)%s*[xX%a]*%s+(.+)$")
    if q and n then
        return n, tonumber(q)
    end

    -- Try to match patterns like "Milk 2"
    local n, q = item_name:match("^(.+)%s+([%d%.]+)$")
    if n and q then
        return n, tonumber(q)
    end

    return item_name, nil
end

local function format_date(ms)
    -- Using ISO 8601 format to match Rust's chrono DateTime<Utc> storage in SQLite
    return os.date("!%Y-%m-%dT%H:%M:%SZ", math.floor(ms / 1000))
end

local function sync_groceries()
    logging.info("Starting OurGroceries sync")

    local db, err = sqlite3.open(DB_PATH)
    if not db then
        logging.error("Failed to open database: " .. tostring(err))
        return
    end

    -- Initialize table
    local _, err = db:exec([[
        CREATE TABLE IF NOT EXISTS groceries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item TEXT NOT NULL,
            crossed_off_at DATETIME NOT NULL,
            quantity REAL,
            UNIQUE(item, crossed_off_at)
        )
    ]])
    if err then
        logging.error("Failed to initialize database: " .. tostring(err))
        return
    end

    -- Get last crossed off at
    -- Note: Since we store dates as ISO 8601 strings, MAX() works lexicographically
    local last_crossed_off = ""
    local rows, err = db:rows("SELECT MAX(crossed_off_at) as max_at FROM groceries")
    if rows then
        local row = rows()
        if row and row.max_at then
             last_crossed_off = row.max_at
        end
    end

    local user = os.getenv("OURGROCERIES_USER")
    local pass = os.getenv("OURGROCERIES_PASS")
    if not user or not pass then
        logging.error("OURGROCERIES_USER or OURGROCERIES_PASS not set")
        return
    end

    local og, err = ourgroceries.new(user, pass)
    if not og then
        logging.error("Failed to login to OurGroceries: " .. tostring(err))
        return
    end

    local lists, err = og:get_lists()
    if not lists then
        logging.error("Failed to get lists: " .. tostring(err))
        return
    end

    local total_inserted = 0

    for _, list in ipairs(lists) do
        logging.debug("Processing list: " .. list.name)
        local items, err = list:get_items()
        if not items then
            logging.error("Failed to get items for list " .. list.name .. ": " .. tostring(err))
            -- "If, after retrying, it failed, skip the remaining lists."
            return
        end

        local list_has_new_items = false
        for _, item in ipairs(items) do
            if item.crossedOff and item.crossedOffAt then
                -- OurGroceries crossedOffAt is in ms
                local crossed_at = item.crossedOffAt

                local crossed_at_str = format_date(crossed_at)

                if crossed_at_str > last_crossed_off then
                    local name, quantity = parse_quantity(item.name)

                    local _, err = db:exec(
                        "INSERT OR IGNORE INTO groceries (item, crossed_off_at, quantity) VALUES (?, ?, ?)",
                        {name, crossed_at_str, quantity}
                    )
                    if not err then
                        total_inserted = total_inserted + 1
                        list_has_new_items = true
                    else
                        logging.error("Failed to insert item: " .. tostring(err))
                    end
                end
            end
        end

        if list_has_new_items then
            local ok, err = list:delete_crossed_off_items()
            if not ok then
                logging.error("Failed to delete crossed off items from list " .. list.name .. ": " .. tostring(err))
            end
        end
    end

    if total_inserted > 0 then
        logging.info("Imported " .. total_inserted .. " items from OurGroceries")
    else
        logging.info("No new items to import")
    end
end

-- Entry point
local cron_expr = os.getenv("OURGROCERIES_CRON")
if cron_expr and cron_expr ~= "" then
    logging.info("Scheduling OurGroceries sync with cron: " .. cron_expr)
    local scheduler = cron.new()
    scheduler:register(cron_expr, sync_groceries)
else
    -- Run once
    sync_groceries()
end
