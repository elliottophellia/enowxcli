---
name: performance
description: "Performance work that is measured: defining the metric and the workload, a baseline, profiling to find where time goes, changing one thing at a time, comparing with statistics, percentiles over averages, latency versus throughput, budgets, and reporting numbers with their conditions. The method and which skill holds each part. Read before any optimisation or performance investigation."
---

# Performance work, measured

Performance work done by default is a guess: code rewritten because it
looked slow, a cache put in front of everything, a loop micro-optimised
that takes 1% of the time, "3x faster" from one run on a laptop, an average
that hides a two-second tail, and a fix that quietly returns wrong answers.
This skill is the method an experienced engineer follows instead: define the
metric and the workload, measure a baseline, find where the time goes,
change one thing, compare with statistics, and report the numbers with the
conditions they were measured under. Each step names the skill that holds
the detail.

## 1. Define the problem as a number

Before reading any code, write down three things.

- **The metric**, the one a user or an operator feels:

  | Complaint | Metric |
  |---|---|
  | The page is slow | LCP, INP and CLS at p75 of real visits (`frontend-performance`) |
  | The API is slow | latency p95 and p99 per endpoint, at a stated request rate |
  | The job takes all night | wall-clock duration for a stated input size |
  | We need more servers | throughput per instance at the latency target |
  | It keeps getting killed | peak resident memory (`performance-memory`) |
  | The download is huge | bundle, binary or payload size, compressed |
  | The first request is slow | cold start, measured apart from warm requests |

- **The target**: a number with its percentile and its condition. "p95
  under 300ms at 200 requests per second with a 50,000-order account", not
  "faster". It comes from an SLO, a budget, a product requirement or the
  owner; if nobody has one, propose one and say it is proposed.
- **The workload**: realistic data sizes (production row counts, the
  largest customer, not a seed of 20 rows), realistic input shapes and
  their distribution, the concurrency, and whether caches are warm or
  cold. Query plans, hit rates and an algorithm's cost all change with
  size; a toy workload optimises the wrong thing.

Done means: the target is met on that workload, and the tests still pass.

## 2. Measure a baseline first

- Measure before changing anything, with the exact command, script or
  query that will measure the after. Keep the raw output, not only a
  summary, and the commit hash it ran on.
- Write the environment down: machine or instance size (CPU model, cores,
  memory), OS, runtime and compiler versions, build mode, configuration
  flags, data size, and anything else running at the time.
- Optimised builds only. A debug build, a framework's development mode
  (React, Next.js, Django with `DEBUG`, Rails), Go's race detector (2 to
  20x slower by its own documentation) or a sanitiser changes the numbers
  by multiples and moves the hot spots.
- Run the baseline three separate times. If the runs differ by as much as
  the improvement you hope for, reduce the noise first
  (`performance-benchmarks`).
- Before and after on the same machine in the same session. Laptops
  throttle on battery and when hot; shared CI runners vary between jobs.
- Capture a profile with the baseline (`performance-profiling`): it is the
  before half of the comparison later.

## 3. Find where the time goes

The hot path is the one the measurement shows, not the one that looks slow.

- Start coarse and split the total into parts. For a request: tracing
  spans (database, external calls, serialisation, CPU, waiting for a pool).
  For a page: the browser's Performance panel. For a command: `time`;
  `real` far above `user + sys` means waiting (I/O, network, locks,
  sleeps), `user + sys` above `real` means several cores worked.
- Then profile the biggest part (`performance-profiling`): a CPU profile
  when a core is busy, an off-CPU or wall-clock view when it waits, a heap
  profile for memory, `EXPLAIN` for a query (`database-queries`).
- Amdahl's law sets the ceiling. A part taking a share `p` of the time,
  made `s` times faster, makes the whole `1 / ((1 - p) + p / s)` times
  faster:

  | Part's share | Part made 2x faster | Part removed entirely |
  |---|---|---|
  | 5% | 1.03x | 1.05x |
  | 20% | 1.11x | 1.25x |
  | 50% | 1.33x | 2x |
  | 80% | 1.67x | 5x |

  A loop at 5% is not worth an afternoon; the part at 80% is the only
  place a large win can come from.

## 4. Change one thing, then measure again

- One hypothesis, one change, the same measurement. Keep it if the number
  moved beyond the noise, revert it if not. Two changes measured together
  hide which one helped and whether the other hurt.
- Cheapest first: configuration and build (release mode, production mode,
  a pool size, a runtime flag, an index) before code, code before
  architecture, architecture before a rewrite in another language.
- Correctness is part of the measurement. Run the tests after every
  change. For risky changes (a cache, concurrency, a new algorithm, a
  denormalised column) add a test comparing old and new results on the
  same inputs, edge cases included: empty, one item, duplicates, the
  largest.
- Revert what did not help. Complexity without a measured gain is a cost
  someone pays later.
- Keep a short log as you go (change, before, after, kept or reverted); it
  becomes the report.
- A change that helps one input shape and hurts another is a trade:
  measure both and say which got slower (small versus large inputs, reads
  versus writes, first request versus steady state).

## 5. Statistics: distributions, not one number

- **Repeat.** One run is an anecdote. Macro measurements: at least 10
  runs, or a load test long enough for thousands of requests.
  Micro-benchmarks: let the tool decide (`performance-benchmarks`).
- **Percentiles over averages.** Report p50, p95 and p99, and the maximum
  when samples are few. An endpoint answering in 50ms 98% of the time and
  in 2s the other 2% has a mean of 89ms: a time no request took, hiding
  the users who waited two seconds.
- **Enough samples.** A p99 from 100 samples is one sample. Collect at
  least 1,000 per p99 you report, and ten times more for p99.9.
- **Never average percentiles.** The mean of ten instances' p95 is not the
  fleet's p95. Merge histograms (HdrHistogram, Prometheus histograms,
  OpenTelemetry exponential histograms) or compute from all the samples.
- **Spread and significance.** Give the spread (interquartile range,
  standard deviation or min to max) with the median. A difference smaller
  than the run-to-run spread is no difference. When it is close, use a
  statistical test: benchstat and criterion report one (hyperfine ships a
  Welch t-test script for its JSON export); otherwise Mann-Whitney U or a
  bootstrap confidence interval.
- **Warm versus cold.** The first request after a deploy pays for JIT
  compilation, empty caches, new connections and a cold disk cache. Say
  which you measured; measure both when real users meet both.
- **Tails multiply.** A request fanning out to 100 backends waits for the
  slowest. If each is slow 1% of the time, 63% of those requests are slow
  (`1 - 0.99^100`): a dependency's p99 becomes its caller's median. Fan-out
  needs fewer calls, tight timeouts or hedged requests.

## 6. Latency, throughput and saturation

- **Latency** is how long one operation takes, what a user waits.
  **Throughput** is how many complete per second, what the system carries.
  Batching raises throughput and adds latency; more parallelism raises
  throughput until something shared saturates.
- **Little's law**: items in the system = arrival rate x time in the
  system. 200 requests per second at 50ms keeps 10 in flight; at 500ms, 100
  are in flight, and pools, threads and memory sized for 10 run out. This
  is how one slow dependency takes a whole service down.
- **Queueing near capacity**: in the simplest queue model, time in the
  system is the service time divided by `(1 - utilisation)`: 2x at 50%
  busy, 5x at 80%, 10x at 90%, 20x at 95%. Latency that is flat, then
  climbs steeply, is a saturated resource, not slow code.
- **The knee**: as load rises, throughput stops growing while p95 and p99
  climb; past it, queues grow without bound and errors start. Find it with
  a load test (`performance-load`) and plan peak traffic at 60 to 75% of
  it.
- A latency without its load is half a number: "p95 120ms at 400 requests
  per second on 8 instances", not "p95 120ms".

## 7. Budgets and regression guards

A speed-up that nobody guards is gone within months.

- Write budgets where the team sees them: endpoint latency (p95 and p99),
  page metrics and JavaScript weight (`frontend-performance`), job
  durations, binary or image size, memory per instance.
- Guard each with the cheapest check that catches the regression:

  | Regression | Guard in CI |
  |---|---|
  | A hot function got slower | benchmarks compared with the base branch (`performance-benchmarks`) |
  | An N+1 crept in | a test asserting an endpoint's query count (Django `assertNumQueries`, Rails `assert_queries_count`, a query counter elsewhere) |
  | A bundle or binary grew | a size limit (size-limit, a byte budget in the build) |
  | Page metrics dropped | Lighthouse CI assertions (`frontend-performance`) |
  | An endpoint slowed under load | a short k6 run with thresholds (`performance-load`) |

- Thresholds sit above the measured noise (often 5 to 10%, more on shared
  runners), never at zero, or the check fails at random and gets ignored.
- In production, alert on the percentile against the SLO, not on the mean
  (`backend-observability`).

## 8. Where the big wins usually are

After the profile points there, in rough order of how often they pay:

| Area | Typical change | Detail |
|---|---|---|
| Algorithm and data structure | a lookup in a nested loop becomes a hash map or set; sort once, not per item; stop re-scanning a list | `code` |
| Database | N+1 removed, a missing index added, fewer rows and columns fetched, keyset pagination | `database-queries`, `database-indexes` |
| Repeated work | computed once per request, pure functions memoised, costly shared results cached, values precomputed on write | `backend-caching` |
| Round trips | multi-get, bulk insert, one `IN` query instead of a loop, pipelining | `performance-backend` |
| Loading everything | rows, lines and responses streamed instead of built in memory | `performance-memory` |
| Independent work in series | independent I/O run concurrently (bounded), CPU work spread over cores | `performance-backend` |
| Payload size | fewer fields, pagination, compression, smaller images, less JavaScript | `frontend-performance` |
| Work on the request path | moved to a background job | `backend-jobs` |
| Build and runtime | release build, production mode, a current runtime, GC settings | `performance-memory` |

Costs worth knowing when estimating, as orders of magnitude on current
hardware:

| Operation | Roughly |
|---|---|
| Main memory reference | 100ns |
| Random 4 KB read from an NVMe SSD | 100 microseconds |
| Round trip inside a data centre | 0.1 to 0.5ms |
| Indexed lookup in a database, round trip included | 0.5 to 1ms |
| Round trip across the internet | 20 to 150ms |
| New HTTPS connection | 2 to 3 extra round trips (TCP, TLS) before the request is sent |

Fifty sequential queries at 1ms is 50ms whatever each query costs.
Micro-optimisations (manual inlining, bit tricks, avoiding a call) come
last, and only inside a loop the profile shows is hot.

## 9. Report numbers with their conditions

```text
Metric:       p95 and p99 latency of GET /api/orders?status=open
Workload:     <tool and script, arrival rate, duration, data size>
Environment:  <instance size, runtime versions, database size, commits before and after>
Method:       <runs or samples, warm or cold, warm-up excluded or not>
Before:       p50 <n> ms, p95 <n> ms, p99 <n> ms, errors <n>%
After:        p50 <n> ms, p95 <n> ms, p99 <n> ms, errors <n>%
Change:       <what changed, in one or two lines>
Trade:        <what got slower or bigger, and for which inputs>
Not measured: <what could not be run, and why>
```

- Absolute numbers with units plus the relative change; never "much
  faster". Only numbers that were measured: no estimates presented as
  results, no figures from a blog post as if they were yours.
- Say what was traded: memory for speed, write cost for read speed (an
  index), freshness for latency (a cache), simplicity for throughput.
- Say plainly what could not be run (no staging, no permission, no
  production-size data) and what the numbers therefore do not show.
- Attach the raw output: benchmark tables, k6 summaries, profiles.

## 10. Which skill for which job

| Job | Skill |
|---|---|
| Where time or memory goes | `performance-profiling` |
| A slow endpoint, service or worker | `performance-backend` |
| Memory that grows, OOM kills, heavy allocation | `performance-memory` |
| Writing a benchmark, claiming a speed-up, CI regression checks | `performance-benchmarks` |
| Capacity, load, stress, soak and spike tests | `performance-load` |
| Web vitals, bundle weight, images, hydration | `frontend-performance` |
| Adding or fixing a cache | `backend-caching` |
| Slow queries, N+1, reading EXPLAIN | `database-queries` |
| Adding, changing or removing indexes | `database-indexes` |
| Allocation strategy, data layout, cache locality | `systems-memory` |
| Animation jank and frame cost | `motion-performance` |

## Check it

- Before and after come from the same command, workload and machine, with
  several runs each, and the change is larger than the spread.
- The profile after the change shows the hot spot shrank and nothing new
  took its place.
- The project's own tests pass, run with `bash`. When a command is refused
  or cannot run (installing a profiler, tuning the CPU, a load test
  against staging), the report says so instead of estimating.
- Every kept change has its own measured gain; the rest were reverted.
- The report states metric, workload, environment, method, trade and what
  was not measured.

## Avoid

Optimising before measuring; optimising the code that looks slow instead
of the code the profile shows; a speed-up from one run; means without
percentiles; averaged percentiles; a latency without its load; a debug
build or development mode measured; several changes measured as one; a
cache added to hide a slow query; a faster result that is wrong; a
micro-benchmark win claimed as a product win; numbers without the workload
and environment; estimates reported as measurements.
