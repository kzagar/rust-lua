-- Database file path
local db_path = "test.db"

-- Open database
local db = sqlite3.open(db_path)

-- Create table if it doesn't exist
print("Creating table 'test_table' if it doesn't exist...")
db:exec([[
    CREATE TABLE IF NOT EXISTS test_table (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        data TEXT
    )
]])

-- Use ORM-like API to add objects
print("\nInserting entries using ORM API...")
for i = 1, 3 do
    local name = "PremiumUser_" .. uuid():sub(1, 4)
    local data = "Some secret data " .. i
    
    -- Create a new object for 'test_table'
    local obj = new_object("test_table", {
        name = name,
        data = data
    })
    
    -- Add it to the database
    db:add(obj)
    print("Added: " .. name)
end

-- Query objects with filters
print("\nQuerying objects with 'like' filter (name starts with 'Premium'):")
local results = db:objects("test_table", { name = like("Premium%") })
for _, obj in ipairs(results) do
    print(string.format("ID: %d, Name: %s, Data: %s", obj.id, obj.name, obj.data))
end

-- Combined filters (e.g., ID greater than 0)
print("\nQuerying objects with combined filters (name like '%User%' and id > 5):")
local results2 = db:objects("test_table", { 
    name = like("%Premi%"),
    id = gt(5)
})
for _, obj in ipairs(results2 or {}) do
    print(string.format("Found: [%d] %s", obj.id, obj.name))
end

-- Close database
db:close()
print("\nDatabase closed.")
