--[[
Leaky Bucket (Multi-Rate) Algorithm - Microsoft Version
------------------------------------------------------
Implements a rate limiter supporting multiple leak rates and burst capacities.
Uses Redis as backend, storing state as packed binary (cmsgpack).

KEYS[1] - Redis key representing the bucket
ARGV[1] - cost (tokens requested for this operation)
ARGV[2..n] - pairs of [flow_rate, burst_capacity] for each rate policy
--]]

-- Enable command replication for Redis Cluster / replicas
redis.replicate_commands()

-- Read inputs
local key = KEYS[1]
local cost = tonumber(ARGV[1]) -- tokens requested (usually 1)

-- Get Redis server time (seconds + microseconds)
local raw_time = redis.call('time')
local now = tonumber(raw_time[1]) + tonumber(raw_time[2]) / 1e6 -- high precision current time

-- Try to read current bucket state (packed binary data)
-- Expected packed data: { last_time, deny_count, token_levels[] }
local okay, last_time, deny_count, token_levels = 
    pcall(cmsgpack.unpack, redis.pcall('get', key))

-- If unpacking failed or key doesn't exist, initialize new bucket
if not okay then
    last_time = now
    deny_count = 0
    token_levels = {}
end

-- Compute time delta since last update (seconds)
local delta = now - last_time

-- Prepare next-state variables
local next_levels = {}        -- token levels after adding cost
local expire_time = 0         -- how long the key should live (seconds)
local free_capacity = math.huge  -- smallest remaining capacity among all limits
local limiting_index = 0         -- which limit was most restrictive

-- Loop through each (flow_rate, burst_capacity) pair
for i = 1, #ARGV / 2 do
    local flow = tonumber(ARGV[2 * i])       -- leak rate (tokens per second)
    local burst = tonumber(ARGV[2 * i + 1])  -- max capacity for this rate
    
    -- Leak tokens that should have leaked since last update
    local previous_level = token_levels[i] or 0
    local leaked_level = math.max(0, previous_level - (delta * flow))
    
    -- Add requested cost (tokens)
    next_levels[i] = leaked_level + cost
    
    -- Calculate how much space remains in this bucket
    local remaining = burst - next_levels[i]
    
    -- Track the smallest remaining capacity (the limiting rate)
    if remaining < free_capacity then
        free_capacity = remaining
        limiting_index = i
    end
    
    -- Estimate TTL: how long until this bucket fully leaks
    expire_time = math.max(expire_time, math.ceil(math.max(burst, next_levels[i]) / flow))
end

-- Decide whether to allow or deny this operation
if free_capacity >= 0 then
    -- ✅ Allowed request:
    -- Store updated state: (time, deny_count, new_levels)
    redis.call('setex', key, expire_time, cmsgpack.pack(now, 0, next_levels))
    -- Return success, remaining capacity, and which limit applied
    return { 1, tostring(free_capacity), limiting_index }
else
    -- ❌ Denied request:
    deny_count = deny_count + cost
    -- Keep the previous levels but update deny count and timestamp
    redis.call('setex', key, expire_time, cmsgpack.pack(now, deny_count, token_levels))
    -- Return failure, total denied tokens, and limiting rate index
    return { 0, tostring(deny_count), limiting_index }
end