---
name: backend-realtime
description: "Real-time features: choosing polling, Server-Sent Events or WebSockets, streaming AI responses, authentication on connect, heartbeats and reconnects with resume, scaling connections across instances with pub/sub, backpressure and limits, message formats, hosted services, and testing. Read before building live updates, chat, notifications or streaming responses."
---

# Real-time

The generated version opens a WebSocket for a notification count that
changes twice a day, keeps the sockets in an array on one server, trusts
whoever connects, reconnects in a tight loop the moment the server restarts,
and loses every message sent while the phone was in a tunnel. This skill
gives the choice of transport, the connection lifecycle, scaling, limits and
tests. One part of the backend; the whole is in the `backend` skill.

## 1. Choose the transport

| Need | Use |
|---|---|
| Changes rarely, or a wait of 10 to 60 s is fine (a report finishing, an order status) | Polling with an `ETag` and `304`, faster while something is pending |
| Server to client streams: notifications, progress, live dashboards, feeds, AI tokens | Server-Sent Events (SSE) |
| Two-way and low latency: chat with typing and presence, collaboration, games, cursors | WebSockets |
| Rooms, presence and fan-out without running the connections yourself | A hosted service (section 9) |
| Unreliable, unordered datagrams over HTTP/3 | WebTransport, rarely: support is still uneven |

- Chat often works as SSE for receiving plus ordinary `POST` requests for
  sending: auth, validation, retries and load balancing stay plain HTTP.
- Polling is not a failure: a 30 s poll answered with `304` costs almost
  nothing and passes every proxy. Long polling (the server holds the request
  25 to 30 s) only as a fallback.
- Serverless functions do not hold WebSockets (Vercel's have none) and bill
  SSE by duration: connections go to a hosted service or a small always-on
  process.

## 2. Server-Sent Events

```text
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
X-Accel-Buffering: no

retry: 5000

id: 1042
event: order.updated
data: {"id":"ord_812","status":"paid"}

: keep-alive
```

- Each event is `id:`, `event:` and `data:` lines ending in a blank line;
  `retry:` sets the browser's reconnect delay in ms; a line starting with `:`
  is a comment, sent every 15 to 30 s so proxies do not close an idle stream.
- `EventSource` reconnects by itself and sends the last id in the
  `Last-Event-ID` header. The server resumes after it, so ids are positions
  in a store (an events table, a Redis stream), not a counter in memory.
  When the id is older than the history kept, send a `reset` event and let
  the client reload its state.
- `EventSource` sends cookies but no custom headers and no body: same-site
  cookie auth works as is. For a bearer token or a `POST` (an AI prompt), use
  `fetch` and read `response.body` as a stream (`@microsoft/fetch-event-source`
  does the parsing and retries).
- Buffering kills streams: `X-Accel-Buffering: no` or `proxy_buffering off`
  in nginx, `proxy_read_timeout` above the heartbeat, no compression
  middleware on `text/event-stream`, and a flush after each event. Idle
  limits to stay under: AWS ALB 60 s by default, Cloudflare 100 s.
- Over HTTP/1.1 a browser allows 6 connections per origin across all tabs:
  a few tabs with streams open freeze the app's other requests. Serve SSE
  over HTTP/2 (streams share one connection) and keep one stream per tab.

## 3. WebSockets

- A maintained library: `ws` in Node, the framework's own
  (`@fastify/websocket`, Hono's `upgradeWebSocket`, FastAPI and Starlette,
  axum's `ws`), `coder/websocket` or `gorilla/websocket` in Go, Action Cable,
  Spring WebSocket, SignalR.
- Authenticate during the upgrade, before accepting. With cookies, also
  check `Origin` against your own origins: a page on another site can open a
  socket to yours, the browser may attach your cookies, and no CORS
  preflight stops it (cross-site WebSocket hijacking). Other clients use a
  ticket: random, single-use, valid 30 to 60 s, fetched with an
  authenticated `POST /realtime/tickets`, passed as `?ticket=`. Never a
  long-lived token in the URL (URLs land in logs); the browser's `WebSocket`
  cannot send an `Authorization` header.
- Heartbeats: the server sends a protocol ping every 20 to 30 s and closes
  the connection when no pong comes back within 10 s. Without this, dead
  connections (a phone that lost signal never sends a close) pile up for
  hours. Browsers answer pings on their own but cannot send them: a client
  that must detect a dead link itself needs an application heartbeat
  message.
- Close codes: `1000` normal, `1001` going away, `1008` policy violation
  (auth failed or revoked), `1009` message too big, `1011` server error,
  `1012` service restart, `1013` try again later; `4000` to `4999` are yours
  (`4001` session expired, say). `1006` is what a client sees when the link
  died without a close frame; it is never sent.
- A maximum message size (64 KB is plenty for chat; `ws` defaults
  `maxPayload` to 100 MiB), with `1009` above it.
- The connection does not outlive the session: when the session expires or
  is revoked, close with `4001` so the client signs in again, instead of a
  removed user still receiving messages for days.

## 4. Reconnect and resume

- The client reconnects with exponential backoff and full jitter: a random
  delay between 0 and min(30 s, 1 s times 2 to the power of the attempt),
  reset after a minute of stable connection. Without jitter, a deploy that
  drops 50,000 connections brings 50,000 reconnects in the same second.
- Every message carries an id and a per-channel sequence number; the client
  keeps the last one it processed.
- On reconnect: authenticate, resubscribe to the same channels, fetch what
  was missed since the last sequence (an HTTP call, or a resume parameter
  answered from the store), and drop duplicates by id.
- The database (or a Redis stream, NATS JetStream) is the record; the socket
  is only the notification. Messages kept only in memory are gone at the
  next restart.
- The interface shows the state (connecting, live, offline) rather than
  pretending the data is live.

## 5. Scaling across instances

Connections live on one instance; events happen on any. A pub/sub bus
between them: the instance that handles the write publishes the channel and
the event, and every instance delivers it to its own local subscribers.

| Backbone | Fits |
|---|---|
| Redis pub/sub | the default; fire and forget, nothing kept for subscribers who were away |
| Redis streams | when instances or clients must replay from an id |
| NATS, JetStream for replay | many services, high fan-out |
| Postgres `LISTEN/NOTIFY` | a small app already on Postgres: payloads under 8,000 bytes (send ids), delivered at commit, one dedicated connection per instance, never through a transaction-mode pooler |

- A chat message: the `POST` stores it in a transaction, publishes after the
  commit, and each instance pushes it to the sockets in that room. Never
  publish inside the transaction: a rollback leaves clients holding a
  message that does not exist.
- Sticky sessions only when state lives on the instance between requests
  (Socket.IO with its long-polling fallback needs them; plain WebSockets and
  SSE do not).
- Per instance: raise the open-file limit (`ulimit -n` 65,536 or more),
  measure memory per idle connection in a load test, and cap connections so
  the load balancer spreads them.

## 6. Backpressure, limits and authorisation

- Each connection has a bounded send queue (a few hundred messages or about
  1 MB; `bufferedAmount` in `ws`). When a slow client fills it, drop what can
  be dropped (typing, cursors, presence), coalesce what is state (send only
  the latest version), and close with `1013` whoever is still behind, to
  resync on reconnect. One slow client never grows server memory unbounded.
- Limits per connection and per user: messages per second (5 to 20 for
  chat), subscriptions per connection (50 to 100), connections per user (5
  to 10), connection attempts per IP per minute. Over a limit: an error
  message, then `1008`.
- Authorise every subscription on the server: may this user join
  `room:42`, follow `order:812`, watch `tenant:12`? The channel name comes
  from the client; the answer comes from the database. When access is
  revoked, remove the subscriber from the channel at once.
- Filter per recipient: an event goes only to connections allowed to see
  it, with only the fields that recipient may see (as in `backend-api`).
- Inbound messages are validated like request bodies: schema, size, rate
  (`backend-security`).

## 7. Message format

```json
{ "type": "message.created", "v": 1, "id": "evt_7f3a9c", "seq": 1042,
  "channel": "room:42", "at": "2026-09-29T10:15:00Z",
  "data": { "id": "msg_881", "body": "On my way", "authorId": "usr_12" } }
```

- One envelope for every message: a `type` as `noun.verb`, a version of the
  payload shape, an id for deduplication, a sequence for order and resume,
  the channel, a UTC time, the data. Clients ignore types they do not know,
  so new types never break old clients.
- JSON by default; MessagePack or Protocol Buffers only when measured rates
  call for it.
- Commands from a client carry their own id and get an acknowledgement
  (`{ "type": "ack", "ref": "cmd_17" }`) or an error with a code (the codes
  of `backend-errors`), so the client knows whether a send worked and can
  retry with the same id without duplicating it.
- Send ids and small changes when clients can fetch the rest; send the
  record when every recipient needs it anyway.

## 8. Streaming AI responses

- SSE over a `fetch` `POST` (the prompt is in the body): one event per chunk
  of the model's stream, flushed as it arrives; a final event with the
  finish reason and usage; an `error` event with a code when the provider
  fails mid-stream (the `200` status has already been sent).
- Keep-alive comments while the model thinks: reasoning models can take 10
  to 60 s before the first token, longer than a proxy's idle limit.
- When the client disconnects (the request's abort signal), abort the
  provider call, so tokens stop being paid for.
- Save the finished answer on the server as it completes, not from the
  client, so a reload shows it. A stream that must survive a reload writes
  its chunks to a store (a Redis stream per message) and the client resumes
  from the last chunk it has; the AI SDK's resumable streams do this.
- The AI SDK (`streamText` and its response helpers) or the provider's SDK
  streaming, rather than parsing the provider's stream by hand. Timeouts,
  token caps and spend per user from `backend-integrations`.

## 9. Hosted services

| Service | For |
|---|---|
| Pusher Channels, Ably | managed pub/sub with channels, presence and auth callbacks; Ably adds history and ordering guarantees |
| Supabase Realtime | apps on Supabase: broadcast, presence, database changes under row-level security |
| Cloudflare Durable Objects | a stateful object per room, WebSockets that hibernate between messages, your code at the edge |
| Liveblocks, or Yjs with Hocuspocus or y-websocket | collaborative editing, cursors and comments on CRDTs; never hand-written merging |
| Socket.IO | a Node server with rooms, acks and reconnects built in; its own protocol and client on both ends, sticky sessions with long polling, a Redis adapter for several instances |

- Your server still authorises: the service calls your endpoint (private
  channel auth, token requests) and you decide which channels a user may
  join. Publishing goes from your server, not from clients, unless the
  service enforces per-channel rights.
- Price it at your expected connections and messages, and keep your own
  message store as the record.

## 10. Shutdown and deploys

- On `SIGTERM`: fail the readiness check so no new connections arrive, close
  sockets with `1012` (or end SSE streams) spread over a few seconds, finish
  sends in flight, then exit, all inside the platform's grace period
  (Kubernetes `terminationGracePeriodSeconds`, 30 s by default).
- Clients treat `1001` and `1012` as reconnect with jitter, `1008` and
  `4001` as sign in first, then reconnect.
- Rolling deploys move every client of an instance: drain instances one at
  a time and let the client's jitter spread the returns.

## Check it

- `curl -N -H 'Accept: text/event-stream' https://host/events` (with the
  session cookie): events arrive one by one as they happen, not in a burst
  at the end (a burst means a buffering proxy or compression), with
  keep-alive comments at the interval.
- With `websocat wss://host/ws` or a small script using the project's
  client: no credentials, a foreign `Origin`, and a ticket used twice are
  each refused with the close code you chose.
- Integration tests with a real client against the app on a random port:
  subscribe to a channel you may not see (refused); miss messages while
  disconnected, reconnect, receive each exactly once; two instances with
  the bus between them deliver across it.
- Kill the server mid-stream: clients come back over spread-out delays and
  resume; the logs show no reconnect storm.
- A client that stops reading: server memory stays flat and the client is
  closed.
- A load test at the expected connection count and message rate (k6 or
  Artillery), watching memory per connection, delivery latency p99 and open
  file descriptors.

## Avoid

WebSockets for what a 30 s poll would do; sockets kept in a global array on
one instance; a connection accepted before it is authenticated; cookie auth
with no `Origin` check; long-lived tokens in the URL; no heartbeats;
reconnecting at once and forever with no jitter; messages that exist only
in memory; publishing before the commit; unbounded send buffers; channels
joined by name with no permission check; a proxy buffering the stream; an AI
stream that keeps running and billing after the user has left.
