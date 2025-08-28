-- Adapted from https://github.com/upstash/ratelimit/blob/3a8cfb00e827188734ac347965cb743a75fcb98a/src/single.ts#L311
local keys = KEYS -- identifier including prefixes
local rcall = redis.call
local maxTokens = tonumber(ARGV[1]) -- maximum number of tokens
local interval = tonumber(ARGV[2]) -- size of the window in milliseconds
local refillRate = tonumber(ARGV[3]) -- how many tokens are refilled after each interval
local now = tonumber(ARGV[4]) -- current timestamp in milliseconds

local results = {}

for i = 1, #keys do
    local key = keys[i]
    local bucket = rcall("HMGET", key, "refilledAt", "tokens")

    local refilledAt
    local tokens

    if bucket[1] == false then
        refilledAt = now
        tokens = maxTokens
    else
        refilledAt = tonumber(bucket[1])
        tokens = tonumber(bucket[2])
    end

    if now >= refilledAt + interval then
        local elapsed = now - refilledAt
        local numRefills = math.floor(elapsed / interval)
        tokens = math.min(maxTokens, tokens + numRefills * refillRate)
        refilledAt = refilledAt + numRefills * interval
    end

    local next_refill = refilledAt + interval

    if tokens == 0 then
        -- Do not mutate TTL/state on empty bucket to avoid unexpected resets under concurrency
        results[key] = {-1, next_refill}
    else
        local remaining = tokens - 1
        local expireAt = math.ceil(((maxTokens - remaining) / refillRate)) * interval

        rcall("HSET", key, "refilledAt", refilledAt, "tokens", remaining)
        rcall("PEXPIRE", key, expireAt)
        results[key] = {remaining, next_refill}
    end
end

-- Redis doesn't support Lua table responses: https://stackoverflow.com/a/24302613
return cjson.encode(results)
