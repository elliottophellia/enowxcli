---
name: backend-jobs
description: "Work outside the request: what belongs in a background job, choosing a queue per stack, payloads, idempotent handlers and deduplication, retries with backoff and jitter, dead letters and alerts, timeouts and heartbeats, concurrency and rate limits, priorities, the outbox pattern, scheduled work that runs once, long workflows, queue metrics, graceful shutdown and tests. Read before sending email, calling slow services, building reports or anything that can take seconds or fail and retry."
---

# Background work

The generated version sends the email inside the request, fires a promise
nobody awaits, retries forever or never, runs the nightly job once per
instance, and cannot say how many jobs are stuck. One part of the backend;
the whole is in the `backend` skill.

## 1. What goes to a job

- Anything a response does not need to wait for, anything slower than about
  a second, and anything that can fail and should be retried: emails and
  notifications, outbound webhooks, reports and exports, image and file
  processing, calls to slow or flaky services, bulk updates.
- The request records what is to be done, answers (`201`, or `202` with where
  to check), and the job does the rest.
- Never "in the background" inside the request process: a promise nobody
  awaits, `setTimeout`, a goroutine or thread tied to nothing. It is lost on a
  restart, never retried, and killed at once on serverless platforms.

## 2. The queue

Use the project's. With none, one that needs no new service where possible:

| Stack | Start with | When it grows |
|---|---|---|
| Node | pg-boss or graphile-worker (PostgreSQL) | BullMQ (Redis): volume, rate limits, flows |
| Python | Procrastinate (PostgreSQL), RQ or Dramatiq (Redis) | Celery for large fleets and routing |
| Go | River (PostgreSQL) | Asynq (Redis) |
| Rails | Solid Queue (the Rails 8 default, database-backed) | Sidekiq (Redis) |
| Laravel | Its queues on the database driver | Redis with Horizon |
| Elixir | Oban (PostgreSQL) | |
| .NET, Java | Hangfire (.NET), JobRunr (Java) | A broker (SQS, RabbitMQ) with consumers |
| Hosted or serverless | The platform's cron (Vercel Cron) with Upstash QStash, Cloud Tasks, SQS or Pub/Sub | |
| Steps over hours or days | Temporal or Inngest (section 11) | |

- A queue in the app's PostgreSQL gives transactional enqueue and one less
  service; it serves most apps until the jobs table is the busiest thing in
  the database.
- The worker runs as its own process, started and stopped with the app, and
  shuts down gracefully: it finishes or releases the job in hand. Its
  database pool is at least its concurrency.

## 3. Payloads

- A job carries ids, not whole records: it loads the current state when it
  runs, which may be different from when it was queued (the order was
  cancelled meanwhile).
- Small JSON with a version: the job name or a `v` field. After a deploy the
  queue still holds old payloads, so new code reads both until they drain; a
  changed meaning is a new job name.
- No secrets and as little personal data as possible: queues show up in
  dashboards and are rarely encrypted. Carry the tenant id and the request
  or trace id of whoever queued it, so logs connect.

## 4. Handlers that can run twice

- Idempotent: running the same job twice has the effect of running it once.
  Key it (`send-receipt:order_812`), check whether the work is already done,
  and use the providers' idempotency keys (`backend-integrations`). Every
  queue here delivers at least once; exactly once is the handler's job.
- Deduplicate at enqueue where the queue can (a BullMQ `jobId`, River's
  unique jobs, Oban's `unique`, an SQS FIFO deduplication id).
- An effect in your database records the key in the same transaction:
  `INSERT INTO processed_jobs (key) VALUES ($1) ON CONFLICT DO NOTHING
  RETURNING key`, and no row back means it already ran.
- An effect outside it (a charge, an email) cannot share the transaction:
  money relies on the provider's idempotency key; an email is sent, then
  marked, accepting a rare duplicate over a lost receipt.
- A time limit per job, and small jobs: a thousand emails are a thousand jobs
  or batches of fifty, not one job that fails at number 612 and starts over.

## 5. Retries and failure

- Retry what can succeed later (a timeout, `429`, `5xx`) with exponential
  backoff and jitter (for example 30 s, 2 min, 10 min, 1 h) and a maximum
  number of attempts (five to ten). Do not retry what never will (a `400`, a
  missing record): fail it at once with the reason.
- Jitter spreads the retries, so a thousand jobs failed by one outage do not
  all come back in the same second: wait a random time up to the capped
  delay (`random() * min(cap, base * 2 ** attempt)`).
- Defaults differ, so set them per job: BullMQ and Celery do not retry
  unless told; Sidekiq and River keep going for about 25 attempts, over
  about three weeks in Sidekiq's case.
- A `429` waits for its `Retry-After`; where the queue allows, without
  spending an attempt (BullMQ's rate limit error, River's `JobSnooze`).

```ts
await receipts.add("send", { orderId }, {
  jobId: `send-receipt-${orderId}`,                  // a second add is ignored
  attempts: 8,
  backoff: { type: "exponential", delay: 30_000 },
  removeOnComplete: { age: 86_400 },
  removeOnFail: { age: 14 * 86_400 },
});

new Worker("receipts", async (job) => {
  const order = await orders.find(job.data.orderId);
  if (!order) throw new UnrecoverableError("order not found"); // no retries
  await mail.sendReceipt(order);                    // provider timeout set
}, { connection, concurrency: 10 });
```

- A job that runs out of attempts goes to a dead-letter list with its error,
  is logged at `error`, and can be retried by hand after a fix.
- Whoever waits on the result can see its state (`queued`, `running`,
  `done`, `failed` with a reason) instead of guessing.

## 6. Dead letters and alerts

- Alert when the dead-letter list grows, when the oldest waiting job is
  older than the queue promises (a minute for sign-in codes, an hour for
  reports), or when failures pass a few percent; each alert says where to
  look.
- SQS: a redrive policy (`maxReceiveCount` around 5) to a dead-letter queue,
  read before the retention (at most 14 days) deletes it.
- Replay after the fix, in batches, from the dashboard or a script; the
  handlers' idempotency makes it safe.

## 7. Timeouts, heartbeats and long jobs

- A timeout per job sized to its work (30 seconds for an email, minutes for
  an export), shorter than the queue's lock or visibility timeout, so a slow
  job fails and retries rather than running twice at once.
- SQS: the visibility timeout above the longest run (at least six times the
  function timeout for a Lambda consumer).
- Long jobs heartbeat or extend their lock (BullMQ renews it while the event
  loop is free; Temporal activities heartbeat). A Node worker that blocks the
  event loop past the lock (30 s by default) sees its job stalled and run
  again: CPU work goes to a sandboxed processor or a worker thread.
- A long job records its progress (the last id done), so a retry resumes
  instead of starting over.

## 8. Concurrency, rate limits and priorities

- Concurrency per worker set on purpose (BullMQ `concurrency`, Sidekiq's
  threads, 5 by default, Celery's `--concurrency`), with a database pool at
  least that large.
- A provider's rate limit is kept by the queue across all workers: BullMQ's
  `limiter: { max: 10, duration: 1000 }`, a token bucket in Redis, or one
  queue per provider with fixed concurrency.
- Fairness: one tenant's 50,000-row import must not starve the rest; split
  it into small jobs, and limit concurrency per tenant.
- Priorities as separate queues with their own workers: `critical` (sign-in
  codes, password resets), `default`, `bulk` (exports, reindexing), so a
  bulk backlog never delays a sign-in code. Priority numbers within one
  queue come second.

## 9. Queue after commit: the outbox

- A job queued before the transaction commits can run against data that
  never existed (the transaction rolled back), and one queued after it can be
  lost if the process dies in between. When both matter (money, orders),
  write the job or event as a row in the same transaction (an `outbox`
  table), and let a dispatcher move committed rows to the queue. A queue in
  the same PostgreSQL database (pg-boss, graphile-worker, River) gets this by
  queueing inside the transaction.
- Otherwise, at the least, queue after the commit, never before it
  (Laravel's `afterCommit`). Solid Queue in its own database (the Rails 8
  default) is not inside the app's transaction.
- The dispatcher loops over `SELECT ... FROM outbox WHERE published_at IS
  NULL ORDER BY id LIMIT 100 FOR UPDATE SKIP LOCKED`, publishes, and marks
  them. It delivers at least once, so consumers stay idempotent.

## 10. Scheduled work

- Recurring work (a daily report, expiring reservations, reminders) is a
  scheduled job in the queue or the platform's cron, not a `setInterval` in the
  web process.
- It runs once when the app runs on several instances: a lock (the queue's
  unique job, an advisory lock) or a single scheduler.
- It picks up what it missed: "expire every reservation older than 15
  minutes", not "expire the ones from the last minute".
- Times of day in the business's time zone, stored and compared in UTC. In
  zones with daylight saving, a job at 02:30 local runs twice or not at all
  on the change days: keep jobs out of 01:00 to 03:00 local.
- The tools: BullMQ's job schedulers (`upsertJobScheduler`), River's
  periodic jobs, Solid Queue's `config/recurring.yml`, Celery beat (exactly
  one beat process), Laravel's scheduler with `onOneServer()` and
  `withoutOverlapping()`, Oban's Cron plugin, Hangfire's `RecurringJob`, a
  Kubernetes CronJob with `concurrencyPolicy: Forbid`. Without one:
  `pg_try_advisory_xact_lock(hashtext('nightly-report'))` in the job's
  transaction, and `false` means another instance has it.

## 11. Long workflows

- Steps that each call something (reserve stock, charge, ship, email): each
  step a job, the state in a table (`orders.status`), the next step queued
  when one finishes; a failed step runs compensation (release the stock,
  refund) rather than leaving half a workflow.
- A workflow engine when it waits for days (a trial ending in 14 days),
  waits on people (approvals), fans out and back in, or has more than a
  handful of steps: Temporal, Inngest, Restate, AWS Step Functions.
  Temporal's workflow code must be deterministic: clocks, randomness and I/O
  go in activities.

## 12. Watching the queues

- Per queue: jobs waiting, the age of the oldest waiting job (what users
  feel), throughput, run time p50 and p95, failures, retries, dead letters.
- A log line per job at start and end: job id, name, attempt, duration,
  outcome, and the request or trace id carried from whoever queued it.
- The queue's own dashboard (Horizon, Sidekiq's web UI, Bull Board, Flower,
  River UI), behind admin sign-in.

## 13. Shutdown and deploys

- On `SIGTERM` a worker stops taking jobs, finishes what it holds within a
  grace period shorter than the platform's (Kubernetes gives 30 seconds by
  default), and leaves the rest to be retried: BullMQ's `worker.close()`,
  Sidekiq's 25-second timeout, Celery's warm shutdown, River's `Stop`,
  `queue:work --timeout`, `horizon:terminate` on deploy.
- A job longer than the grace period is split or checkpointed: every deploy
  interrupts it.
- Celery acknowledges a task when it starts, so a killed worker loses it:
  idempotent tasks set `acks_late=True` and `reject_on_worker_lost=True`,
  and a Redis broker's `visibility_timeout` (1 hour by default) must exceed
  the longest task.

## 14. Testing jobs

- The handler called directly with a payload against a real database: the
  effect happens; called twice, it happens once.
- A permanent failure (the provider's `400`) fails without retries; a
  temporary one (`503`) retries.
- Enqueueing asserted where it happens (Laravel's `Queue::fake()`, Rails's
  `assert_enqueued_with`, River's `rivertest.RequireInserted`); a rolled-back
  transaction leaves no job; scheduled jobs run under a controlled clock
  (`backend-testing`).

## Check it

- Start the worker beside the app, queue a job, and find its start and end
  lines with id, duration and outcome. Send `SIGTERM` mid-job: it finishes
  or comes back.
- Make the provider fake answer `503`: attempts back off; `400`: one attempt,
  then the dead-letter list. Two scheduler instances run a cron job once.

## Avoid

An email sent inside the request; a promise nobody awaits; a job that does
the work twice when retried; a job that holds a whole record from an hour
ago; retries forever, or none; a failed job nobody can see; a job queued
before its transaction commits; a cron in the web process that runs once per
instance; one queue where a bulk import delays sign-in codes; a payload with
secrets; a worker killed mid-job with no retry; a queue nobody measures.
