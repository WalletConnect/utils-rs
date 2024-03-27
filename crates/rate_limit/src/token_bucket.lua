-- Adapted from https://github.com/upstash/ratelimit/blob/3a8cfb00e827188734ac347965cb743a75fcb98a/src/single.ts#L311
local keys = KEYS -- identifier including prefixes
local maxTokens = tonumber(ARGV[1]) -- maximum number of tokens
local interval = tonumber(ARGV[2]) -- size of the window in milliseconds
local refillRate = tonumber(ARGV[3]) -- how many tokens are refilled after each interval
local now = tonumber(ARGV[4]) -- current timestamp in milliseconds

local results = {}
for i, key in ipairs(KEYS) do
    local bucket = redis.call("HMGET", key, "refilledAt", "tokens")
    local refilledAt = (bucket[1] == false and tonumber(now) or tonumber(bucket[1]))
    local tokens = (bucket[1] == false and tonumber(maxTokens) or tonumber(bucket[2]))

    if tonumber(now) >= refilledAt + interval then
        tokens = math.min(tonumber(maxTokens), tokens + math.floor((tonumber(now) - refilledAt) / interval) * tonumber(refillRate))
        refilledAt = refilledAt + math.floor((tonumber(now) - refilledAt) / interval) * interval
    end

    if tokens > 0 then
        tokens = tokens - 1
        redis.call("HSET", key, "refilledAt", refilledAt, "tokens", tokens)
        redis.call("PEXPIRE", key, math.ceil(((tonumber(maxTokens) - tokens) / tonumber(refillRate)) * interval))
        results[key] = {tokens, refilledAt + interval}
    else
        results[key] = {-1, refilledAt + interval}
    end
end
-- Redis doesn't support Lua table responses: https://stackoverflow.com/a/24302613
return cjson.encode(results)
