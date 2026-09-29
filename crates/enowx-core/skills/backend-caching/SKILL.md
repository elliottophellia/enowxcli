---
name: backend-caching
description: "Caching on the server without serving wrong data: what is worth caching, HTTP caching with Cache-Control and ETags, CDN and shared caches, an application cache in Redis or memory, keys and invalidation, stampede protection, per-user data kept out of shared caches, and measuring hit rates. Read before adding a cache or when data is slow or stale."
---

# Caching

The generated cache is `redis.set(key, JSON.stringify(result))` with no
expiry, a key that forgets the tenant or the language, nothing deleted when
the record changes, a CDN rule that caches `/account` for everyone, and a
hundred requests rebuilding the same value the second it expires. A cache
that serves wrong data is worse than none. This skill gives the order to
decide in, the headers, the key and invalidation rules, and the checks. One
part of the backend; the whole is in the `backend` skill.

## 1. Decide before caching

- A cache earns its place when the value is read often, costs real time or
  money to produce, and may be a little stale. All three. Written as often
  as read, cheap to build, or wrong the moment it changes (a balance, stock
  at checkout, what a user may do): do not cache it.
- Make the source fast first: an index or a rewrite for the slow query
  (`database-queries`), the N+1 removed (`backend-data`). A cache over a 2 s
  query keeps a 2 s miss, a stampede at every expiry and invalidation bugs;
  the query fixed to 20 ms may need no cache at all.
- For each cached thing, write down the key, what it depends on, the TTL (how
  stale it may be) and what deletes it. When nothing deletes it, the TTL is
  the only guarantee: keep it short.

| What | Where | Lifetime |
|---|---|---|
| Hashed static assets (`app.3f9a1c.js`) | browser and CDN | 1 year, `immutable` |
| Public pages and public API reads | CDN, revalidated | 1 to 10 min, or hours with purges |
| Expensive computed values (a dashboard aggregate, a price list) | application cache | 1 to 15 min plus deletes on write |
| Slow or rate-limited provider calls (rates, geocoding) | application cache | what the provider allows |
| Per-user responses | browser (`private`), or app cache keyed by user | short |
| Money, stock at checkout, permissions, read-after-write data | nowhere | |

## 2. HTTP caching

| Response | `Cache-Control` |
|---|---|
| Hashed static asset | `public, max-age=31536000, immutable` |
| Public read, the same for everyone | `public, max-age=0, s-maxage=300, stale-while-revalidate=60` |
| HTML that must change on deploy | `no-cache` with an `ETag` |
| Per-user data | `private, no-cache` with an `ETag`, or `private, max-age=30` |
| Tokens, account, payment and admin pages | `no-store` |
| `5xx` | never cached; `404` at most 60 s |

- `no-cache` means store but revalidate every time; `no-store` means never
  store. `s-maxage` applies to shared caches only and overrides `max-age`
  there; `stale-while-revalidate=N` serves the old copy for N seconds while
  it refetches; `stale-if-error=N` keeps serving it while the origin fails.
  `CDN-Cache-Control` (RFC 9213) sets a lifetime only CDNs read.
- An `ETag` on every cacheable `GET`; a request whose `If-None-Match` holds
  it gets `304 Not Modified` with no body. Build the tag from what the
  response depends on (`W/"product-812-v17"`, from the id and a version or
  `updated_at`), so the 304 is decided without rendering the body. Body-hash
  ETags (Hono's `etag()`, Express's default, Spring's
  `ShallowEtagHeaderFilter`) save bandwidth, not work; Rails's `fresh_when`
  and `stale?` use the record's version.

```ts
app.get("/products/:id", async (c) => {
  const p = await products.get(c.req.param("id"));
  if (!p) return c.notFound();
  const etag = `W/"product-${p.id}-v${p.version}"`;
  c.header("ETag", etag);
  c.header("Cache-Control", "public, max-age=0, s-maxage=300, stale-while-revalidate=60");
  if (c.req.header("If-None-Match")?.split(/\s*,\s*/).includes(etag)) return c.body(null, 304);
  return c.json(toProductOut(p));
});
```

- `Vary` names the request headers that change the response:
  `Accept-Encoding`, `Accept-Language` when negotiated, and `Origin` when the
  CORS headers are set per origin (without it, a CDN hands one origin's
  `Access-Control-Allow-Origin` to the others). Never `Vary: Cookie` or
  `Vary: Authorization` to make per-user pages cacheable at a CDN.
- A shared cache stores a response to a request carrying `Authorization`
  only when the response says `public`, `s-maxage` or `must-revalidate`
  (RFC 9111). So `public` on an authenticated endpoint is an order to share
  it: never.
- A response with `Set-Cookie` must not be cached by a CDN, and most refuse
  to. A framework that refreshes the session cookie on every response makes
  every page uncacheable, and a CDN rule forcing it caches the cookie.
  Public cacheable routes send no cookie.

## 3. CDN and shared caches

- The CDN follows the origin's `Cache-Control`; configure it to cache
  nothing by extension or path guesswork. Cloudflare caches static
  extensions by default and HTML or JSON only through a Cache Rule.
- Web cache deception: a rule that caches everything ending in `.css`, plus
  an app that answers `/account/settings/x.css` with the account page, hands
  the next visitor someone's account. The origin answers unknown paths with
  `404` and marks private pages `no-store`.
- The cache key: host, path, and only the query parameters that change the
  response, sorted. Tracking parameters (`utm_*`, `gclid`, `fbclid`) dropped,
  or every click is its own miss.
- Purge on change: by URL for one page, by tag for everything a record
  appears in (`Cache-Tag` on Cloudflare, `Surrogate-Key` on Fastly,
  `revalidateTag` in Next.js). Tag each response with the records it holds
  (`product-812`, `category-3`) and purge after the commit, from a job
  (`backend-jobs`). With purges, CDN lifetimes can be hours; without, minutes.

## 4. The application cache

- Redis or Valkey (the same protocol and clients) for values shared by
  instances, on an instance of its own: `maxmemory` with
  `maxmemory-policy allkeys-lru` (or `allkeys-lfu`). Queues, sessions and
  rate limits live on another with `noeviction`: evicting a queue's keys
  loses jobs (BullMQ requires `noeviction`).
- In-process memory for small, hot, rarely changing data (settings, flags, a
  currency list): an LRU with a size bound and a TTL (`lru-cache`,
  `cachetools.TTLCache`, Caffeine, `IMemoryCache`, `moka`, `golang-lru`'s
  expirable cache). Each instance holds its own copy, so a change shows at
  different moments: TTLs of 5 to 60 s, or a pub/sub message that clears it
  everywhere. Never an unbounded `Map` or dict: a memory leak with a hit rate.
  Memory in front of Redis only for the few hottest keys, after measuring.
- Cache-aside is the default: read the cache; on a miss, load from the
  source, store with a TTL, return. Writes go to the database, then delete
  the key (section 6). A cache that is down or slow means going to the
  source, never an error page:

```ts
// ioredis, created with { commandTimeout: 100 } so a slow cache fails fast.
async function cached<T>(key: string, ttl: number, load: () => Promise<T | null>) {
  try {
    const hit = await redis.get(key);
    if (hit !== null) return JSON.parse(hit) as T | null;
  } catch (err) {
    log.warn({ err, key }, "cache read failed"); // fail open
  }
  const value = await load();
  try {
    // jitter() spreads expiry (section 7); a missing record is kept 30 s.
    await redis.set(key, JSON.stringify(value), "EX", jitter(value === null ? 30 : ttl));
  } catch (err) {
    log.warn({ err, key }, "cache write failed");
  }
  return value;
}
```

- Memoise within a request: the record read in five places of one request is
  loaded once (a per-request map, DataLoader, React's `cache()` on the
  server). It dies with the request, so it needs no invalidation.

## 5. Keys

- Namespaced and versioned: `app:version:entity:id` (`shop:v4:product:812`).
  Bump the version when the cached shape or the code building it changes, so
  a deploy never reads values written by the old code, and old and new
  instances in a rolling deploy do not overwrite each other.
- Every input that changes the result is in the key: tenant, locale,
  currency, the caller's role or plan when the value depends on it, feature
  flags, each query parameter normalised (defaults filled, keys sorted). A
  long input goes in as a hash (`shop:v4:search:` plus the SHA-256 of the
  normalised query); values that can contain `:` are hashed too, so two
  inputs never build the same key.
- No `KEYS pattern` in production (it blocks Redis while it walks every
  key): delete groups through a generation number (section 6).

## 6. Invalidation

- Every entry has a TTL, the ceiling on how wrong it can be.
- On a write, delete the affected keys after the transaction commits (in the
  same code path, or from the outbox, `backend-jobs`). Delete, do not
  overwrite: a write that sets the new value races a reader setting an older
  one. A delete before the commit lets a reader cache the old row again.
- Lists and aggregates that many writes touch get a generation counter,
  read into the key and incremented by writes; old keys age out by TTL:

```text
gen    = GET  shop:v4:gen:catalogue:{shopId}       (missing = 0)
key    =      shop:v4:catalogue:{shopId}:g{gen}:page:{n}
write -> INCR shop:v4:gen:catalogue:{shopId}
```

- A reader that loaded the old row just before the commit can still store it
  after the delete. The TTL bounds that window; where seconds of it matter,
  delete again a second later (a delayed job), or do not cache that data.
- Other instances' memory caches are cleared by a pub/sub message (Redis
  pub/sub, Postgres `LISTEN/NOTIFY`); other services' caches by consuming
  change events. A deploy that bumps key versions starts cold: warm the
  hottest keys from a job.

## 7. Stampedes

When a hot key expires, every request that misses rebuilds it at once and
the database takes the whole load.

- Jitter every TTL by 10 to 20%, so keys written together do not expire
  together: `Math.round(ttl * (0.9 + Math.random() * 0.2))` (`EX` takes
  whole seconds).
- Single-flight in the process: concurrent misses for one key share one
  load (a map of in-flight promises, Go's `singleflight`, Caffeine's
  `get(key, loader)`, .NET's HybridCache).
- Across instances, a short lock for the rebuild:
  `SET lock:{key} {token} NX PX 5000`. The winner rebuilds; the others serve
  the stale value, or wait 50 to 100 ms and read again. Release by
  compare-and-delete of the token (a Lua script), never a bare `DEL`.
- Serve stale while refreshing: store a soft expiry inside the value and a
  longer Redis TTL; past the soft expiry, return the value and refresh in
  the background under the lock. Laravel's `Cache::flexible`, Rails's
  `race_condition_ttl` and HybridCache give a form of this.
- For the very hottest keys, early probabilistic refresh (XFetch): refresh
  when `now - delta * beta * ln(random()) >= expiry`, with `delta` the time
  the last rebuild took and `beta` 1.

## 8. Values

- JSON (or MessagePack) of a plain DTO holding the fields needed. Never a
  serialised ORM object or a language-native format (`pickle`, PHP
  `serialize`): they break across versions, and whoever can write the cache
  can run code in the reader.
- Small values, under about 100 KB: a large one stalls Redis while it is
  sent. Cache pages of a list, not the whole list.
- Not-found cached briefly (30 to 60 s) when misses are frequent (a scraper
  probing ids), and deleted when the record is created.

## 9. Per-user and permission-dependent data

- What depends on who asks stays out of shared caches: `private` or
  `no-store` in HTTP, the user or permission set in the application cache
  key (`shop:v4:cart:user:{id}`). Better still, cache what is the same for
  everyone and apply the per-user part after the read (the product from the
  cache, the user's price group and visibility computed per request).
- A tenant id in every key of a multi-tenant app, and a test proving two
  tenants never read each other's value. Signed-in and signed-out variants
  are different keys.

## 10. The framework's cache

Use the project's cache API rather than a second client: Next.js tags and
`revalidateTag` (`backend-stack-next`); Laravel's `Cache::remember` and
`Cache::flexible`, tags on Redis; Rails's `Rails.cache.fetch` on Solid Cache
or Redis (`backend-stack-rails`); Django's cache framework with its Redis
backend; Spring's `@Cacheable` with Caffeine or Redis and explicit keys
(`backend-stack-java`); ASP.NET Core's HybridCache and output caching
(`backend-stack-dotnet`).

## 11. Measure it

- Hit ratio per key prefix, counted in the app; `INFO stats` gives
  `keyspace_hits`, `keyspace_misses` and `evicted_keys` for the instance. A
  hot-path cache under about 80% is worth a look: TTL too short, keys too
  specific, or memory too small (evictions rising).
- Cache call latency: p50 under 1 ms, p99 under 5 ms on the same network;
  more means big values or a saturated instance (`redis-cli --bigkeys`,
  `MEMORY USAGE key`).
- CDN: the hit ratio in its analytics, and `cf-cache-status`, `x-cache`,
  `x-vercel-cache` or `age` on responses.

## Check it

- `curl -sI` a cached URL twice: the headers you meant (`cache-control`,
  `etag`, `vary`), the CDN status going from MISS to HIT, no `set-cookie`;
  with `-H 'If-None-Match: <etag>'` it answers `304`.
- Signed in as user A, then as user B, then signed out, on the same URLs: no
  response of A's reaches B or the anonymous visitor at any layer. The same
  across two tenants.
- Change a record and read it: fresh at once where the write deletes the
  key, within the promised TTL elsewhere.
- Stop Redis in development: the app still answers, from the source.
- `redis-cli --scan --pattern 'shop:v4:*' | head` shows real keys, each with
  a TTL; tests cover the key builder and the delete on write.

## Avoid

Caching before fixing the query; a `SET` without expiry; keys missing the
tenant, locale or version; `public` or `s-maxage` on anything that depends
on the user; a CDN caching by file extension; `Set-Cookie` on cached
responses; overwriting the cache on write instead of deleting after the
commit; `KEYS` in production; one Redis with `allkeys-lru` holding both the
cache and the queue; pickled or serialised ORM objects; every key expiring
in the same second; an unbounded in-memory map; a cache outage that takes
the site down with it.
