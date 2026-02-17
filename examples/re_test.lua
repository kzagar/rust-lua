local r = re.compile([[g_teamId = "(.*)";]])
local text = [[var g_teamId = "some-team-id-123";]]
local m = r:match(text)

if m then
    print("Match found!")
    print("Whole match: " .. m[0])
    print("Group 1: " .. m[1])
    if m[1] == "some-team-id-123" then
        print("Regex test PASSED")
    else
        print("Regex test FAILED (wrong group value)")
    end
else
    print("Match NOT found!")
    print("Regex test FAILED")
end

-- Test named captures
local r2 = re.compile([[(?P<key>\w+)=(?P<value>\w+)]])
local m2 = r2:match("foo=bar")
if m2 and m2.key == "foo" and m2.value == "bar" then
    print("Named captures test PASSED")
else
    print("Named captures test FAILED")
end
