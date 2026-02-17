local results = {}
local start_time = now()

-- do_task(duration, description) returns a closure that simulates a task
function do_task(duration, description)
    return function()
        print(string.format("[%.4f] starting %s", now() - start_time, description))
        wait(duration)
        print(string.format("[%.4f] %s is done", now() - start_time, description))
        results[description] = "Done (duration: " .. duration .. "s)"
    end
end

-- parallel(...) is now implemented in Rust and available globally
-- It takes any number of functions (closures) and executes them concurrently

-- Demonstration: Individual closures
print("[Parallel] Starting 3 tasks using individual arguments...")
parallel(
    do_task(0.5, "parallel-1"),
    do_task(1.0, "parallel-2"),
    do_task(1.5, "parallel-3")
)

-- Demonstration: Flattening (list of closures)
print("\n[Parallel] Starting tasks using a list (flattening)...")
local task_list = {
    do_task(0.5, "list-1"),
    do_task(1.0, "list-2")
}
parallel(task_list, do_task(0.7, "solo"))

-- Demonstration: Sequential execution
print("\n[Sequential] Executing tasks one by one...")
sequential(
    do_task(0.5, "seq-1"),
    do_task(0.5, "seq-2")
)

print("\n[Parallel] Mixed delays (parallel test)...")
parallel(
    do_task(1, "delay-1"),
    do_task(2, "delay-2")
)

print("[Status] All demonstrations completed.")

print("\n" .. string.rep("=", 60))
print(string.format("%-40s | %s", "Task", "Result"))
print(string.rep("-", 60))
for task, result in pairs(results) do
    print(string.format("%-40s | %s", task, result))
end
print(string.rep("=", 60))
