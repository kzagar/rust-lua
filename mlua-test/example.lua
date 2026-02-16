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

-- Insert some entries with random strings
print("Inserting entries...")
for i = 1, 5 do
    local name = "User_" .. uuid():sub(1, 8)
    local data = uuid()
    db:exec(string.format("INSERT INTO test_table (name, data) VALUES ('%s', '%s')", name, data))
end

-- Query the table
print("Querying entries:")
local row_iter = db:rows("SELECT id, name, data FROM test_table ORDER BY id DESC LIMIT 10")
for row in row_iter do
    print(string.format("[%d] Name: %s, Data: %s", row.id, row.name, row.data))
end

-- Close database
db:close()
print("Database closed.")
