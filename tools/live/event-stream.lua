-- Atomic, retry-safe producer. KEYS: stream, metadata, producer receipt.
-- ARGV: source (game boot/log follower), type, codepage, seq, JSON, seq, JSON...
-- One bounded native flush, at most 256 events. Never depend on subscribers.
local last = tonumber(redis.call('GET', KEYS[3]) or '0')
local n = tonumber(redis.call('HGET', KEYS[2], 'n') or '0')
local epoch = redis.call('HGET', KEYS[2], 'epoch')
if not epoch then
    local now = redis.call('TIME')
    epoch = ARGV[1] .. ':' .. now[1] .. ':' .. now[2]
end
for i = 4, #ARGV, 2 do
    local seq = tonumber(ARGV[i])
    if seq > last then
        n = n + 1
        redis.call('XADD', KEYS[1], 'MAXLEN', '~', '10000', '*',
            'n', tostring(n), 'epoch', epoch, 'source', ARGV[1],
            'type', ARGV[2], 'codepage', ARGV[3], 'data', ARGV[i + 1])
        last = seq
        -- Keep each accepted prefix retry-safe even if a later row errors.
        redis.call('HSET', KEYS[2], 'n', tostring(n), 'epoch', epoch)
        redis.call('SET', KEYS[3], tostring(last), 'EX', '1209600')
    end
end
return last
