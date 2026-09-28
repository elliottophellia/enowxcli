---
name: backend-jobs
description: "Work outside the request: what belongs in a background job, choosing a queue, idempotent handlers, retries with backoff, dead letters, the outbox pattern, scheduled work that runs once. Read before sending email, calling slow services, building reports or anything that can take seconds or fail and retry."
---

# Background work

One part of the backend; the whole is in the `backend` skill.

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

- Use the project's. With none, one that needs no new service where
  possible:
  - PostgreSQL already there: pg-boss or graphile-worker (Node), Procrastinate
    (Python), River (Go);
  - Redis already there: BullMQ (Node), RQ or Dramatiq (Python), Asynq (Go);
  - Laravel: its queues, with the database driver to start and Horizon with
    Redis later;
  - a hosted platform: its scheduler and a hosted queue (Vercel Cron with
    Upstash QStash, Cloud Tasks, SQS).
- The worker runs as its own process, started and stopped with the app, and
  shuts down gracefully: it finishes or releases the job in hand.

## 3. Handlers

- Idempotent: running the same job twice has the effect of running it once.
  Key it (`send-receipt:order_812`), check whether the work is already done,
  and use the providers' idempotency keys (`backend-integrations`).
- A job carries ids, not whole records: it loads the current state when it
  runs, which may be different from when it was queued (the order was
  cancelled meanwhile).
- A time limit per job, and small jobs: a thousand emails are a thousand jobs
  or batches of fifty, not one job that fails at number 612 and starts over.

## 4. Retries and failure

- Retry what can succeed later (a timeout, `429`, `5xx`) with exponential
  backoff and jitter (for example 30 s, 2 min, 10 min, 1 h) and a maximum
  number of attempts (five to ten). Do not retry what never will (a `400`, a
  missing record): fail it at once with the reason.
- A job that runs out of attempts goes to a dead-letter list with its error,
  is logged at `error`, and can be retried by hand after a fix.
- Whoever waits on the result can see its state (`queued`, `running`,
  `done`, `failed` with a reason) instead of guessing.

## 5. Queue after commit: the outbox

- A job queued before the transaction commits can run against data that
  never existed (the transaction rolled back), and one queued after it can be
  lost if the process dies in between. When both matter (money, orders),
  write the job or event as a row in the same transaction (an `outbox`
  table), and let a dispatcher move committed rows to the queue. A queue in
  the same PostgreSQL database (pg-boss, graphile-worker, River) gets this by
  queueing inside the transaction.
- Otherwise, at the least, queue after the commit, never before it.

## 6. Scheduled work

- Recurring work (a daily report, expiring reservations, reminders) is a
  scheduled job in the queue or the platform's cron, not a `setInterval` in the
  web process.
- It runs once when the app runs on several instances: a lock (the queue's
  unique job, an advisory lock) or a single scheduler.
- It picks up what it missed: "expire every reservation older than 15
  minutes", not "expire the ones from the last minute".
- Times of day in the business's time zone, stored and compared in UTC.

## Avoid

An email sent inside the request; a promise nobody awaits; a job that does
the work twice when retried; a job that holds a whole record from an hour
ago; retries forever, or none; a failed job nobody can see; a job queued
before its transaction commits; a cron in the web process that runs once per
instance.
