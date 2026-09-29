---
name: performance-profiling
description: "Finding where time and memory go: sampling versus instrumenting profilers, CPU, heap, allocation, lock and I/O profiles, flame graphs and how to read them, the tools for Node, browsers, Python, Go, Rust, the JVM, .NET and native code, profiling in production safely, and turning a profile into a change. Read before optimising anything, and when something is slow for no obvious reason."
---

# Profiling: where time and memory go

Without a profile, optimisation is a guess: timers sprinkled around the
function that looks slow, a debug build profiled, a flame graph's colours
read as heat and its left-to-right order as time, a library frame
"optimised" when the real question is why the code calls it ten thousand
times. This skill picks the profile for the symptom, gives the commands
per runtime, and shows how to read the result and turn it into a change.
Measuring before and after is in `performance`.

## 1. Pick the profile from the symptom

| Symptom | Look with |
|---|---|
| A core at 100%, slow | CPU sampling profile (on-CPU) |
| Slow, but the CPU mostly idle | wall-clock or off-CPU profile, tracing spans, lock and I/O profiles |
| Memory grows or is too high | heap snapshots over time, heap profile (`performance-memory`) |
| High GC time, p99 spikes | allocation profile, GC logs |
| Threads or tasks waiting on each other | lock, mutex or block profile; thread dumps |
| A slow query | EXPLAIN and the database's statement statistics (`database-queries`) |
| A janky page or a slow interaction | the browser's Performance panel (`frontend-performance`) |

A minute with the operating system tells you the row: on Linux `top -H`,
`vmstat 1` and `iostat -x 1`; anywhere, `time <command>`, where `real` far
above `user + sys` means the process waits (a CPU profile will look empty).

## 2. Sampling, instrumenting and tracing

- **Sampling** records stacks at a fixed rate: 99 Hz in production (an odd
  rate avoids lockstep with periodic work), 1,000 Hz or more locally. The
  overhead is low and even across functions, so it is safe in production;
  it is statistical, so collect thousands of samples (30 to 60s under
  load) and expect it to miss short, rare events.
- **Instrumenting** wraps every call (cProfile, manual timers, spans):
  exact counts, but overhead grows with the number of calls, so small hot
  functions look worse than they are. Use it for counts, not proportions.
- **Tracing** records the timeline of one run or request: order,
  concurrency and gaps (the Performance panel, `go tool trace`,
  OpenTelemetry spans, JFR events). A profile aggregates; a trace shows
  sequence and waiting.

## 3. On-CPU, off-CPU and wall-clock

A CPU profile shows only time spent running: time blocked on a socket, a
disk, a lock, a pool or the database is invisible in it. Use instead:

- Wall-clock profiles, which sample every thread, running or not: py-spy
  with `--idle`, async-profiler `-e wall`, pyinstrument (wall-clock by
  default), fgprof for Go.
- Off-CPU analysis on Linux: bcc's `offcputime` records where threads
  blocked and for how long, drawn as a flame graph (section 5).
- The runtime's own waiting views: Go's block and mutex profiles, JFR
  monitor and park events, async-profiler `-e lock`, tokio-console for
  async tasks, Node's event loop delay; and spans, for the gaps and the
  calls made in series that could run concurrently (`performance-backend`).

## 4. Reading a flame graph

- Each box is a function; the boxes above it are what it called. Width is
  the share of samples in it and everything it called (total time); height
  is only stack depth.
- In a classic flame graph the x-axis is sorted alphabetically to merge
  stacks, so left to right is not time; in a flame chart (the Performance
  panel, speedscope's Time Order view) it is. Colour is random or by kind
  of code, not heat, unless the legend says otherwise.
- **Self time** is the top edge of a box not covered by its children: the
  function itself did the work. A wide box with narrow children is hot
  itself; a wide box whose width is all children is only on the path.
- Find the wide plateaus at the top, then walk down the stack to the first
  frame in your own code: that is where the decision to do the work was
  made, and usually what you can change.
- One function in many thin towers under different callers adds up
  unseen: a bottom-up view (DevTools Bottom-Up, flamegraph.pl `--reverse`,
  speedscope's Sandwich) ranks functions by self time across all callers.
  Compare before and after with a diff (`difffolded.pl`, Pyroscope,
  `pprof -diff_base`).
- `[unknown]` frames and broken stacks: build with symbols and frame
  pointers (`-fno-omit-frame-pointer`, Rust `-C force-frame-pointers=yes`)
  or unwind with DWARF (`perf record --call-graph dwarf`); JIT runtimes
  need a perf map (`node --perf-basic-prof`). Inlined functions vanish
  into their callers; line views (pprof `list`, `perf annotate`) find them.

## 5. The tools, by runtime

Profile an optimised build with a realistic workload running: never a
debug build, a development server or an idle process.

**Node.js**

```bash
node --cpu-prof --cpu-prof-dir=./prof app.js   # open in DevTools or with npx speedscope
node --heap-prof app.js                         # sampled allocations, .heapprofile
node --inspect app.js                           # chrome://inspect: Performance, Memory
```

- `--cpu-prof` writes on a normal exit: a server needs a `SIGINT` handler
  that calls `process.exit()`. clinic.js (`clinic flame`) and 0x draw a
  flame graph in one command; check they support the project's Node first.
- A blocked event loop is CPU work on the main thread. Measure it with
  `perf_hooks.monitorEventLoopDelay()`: a p99 in the tens of milliseconds
  means every request queues behind that work; move it to `worker_threads`
  or a job.

**Browsers**: the Performance panel with CPU throttling (4x or 6x), then
the Main track (a flame chart), Bottom-Up for self time, and long tasks
(over 50ms, flagged red). For a slow interaction, INP attribution splits
input delay, processing and presentation; the `web-vitals` attribution
build and the Long Animation Frames API report it from real users. React
DevTools' Profiler shows which components rendered and why.

**Python**

```bash
py-spy top --pid <pid>                                  # live, no restart
py-spy record -o prof.svg --pid <pid> --duration 30     # add --idle for wall-clock
py-spy dump --pid <pid>                                 # every thread's stack, for a hang
python -m cProfile -o out.prof app.py && snakeviz out.prof
pyinstrument app.py                                     # wall-clock call tree, async-aware
scalene app.py                                          # Python, native and system time per line
```

py-spy attaches without code changes (sudo on macOS, `SYS_PTRACE` in a
container); `--native` adds C extension frames, `--subprocesses` follows
gunicorn or multiprocessing workers. cProfile instruments: right for call
counts, inflated for small functions. line_profiler (`kernprof -l -v`)
times each line of functions marked `@profile`; Python 3.12+ works with
Linux perf via `python -X perf`.

**Go**: register the handlers with `import _ "net/http/pprof"` and serve
them on a private port, `go http.ListenAndServe("localhost:6060", nil)`. If
the public server uses `http.DefaultServeMux`, the import exposes them there.

```bash
go tool pprof -http=:8081 'http://localhost:6060/debug/pprof/profile?seconds=30'
go tool pprof -http=:8081 http://localhost:6060/debug/pprof/heap     # in use
go tool pprof -http=:8081 http://localhost:6060/debug/pprof/allocs   # all allocations
curl -o trace.out 'http://localhost:6060/debug/pprof/trace?seconds=5' && go tool trace trace.out
go test -run='^$' -bench=Parse -cpuprofile cpu.out -memprofile mem.out ./parser
```

- Block and mutex profiles stay empty until enabled with
  `runtime.SetBlockProfileRate(n)` and `runtime.SetMutexProfileFraction(n)`
  (a smaller `n` records more and costs more).
- In pprof: `top` (self), `top -cum` (total), `list Func` (per line), the
  flame graph under View. `go tool trace` shows scheduler latency, GC
  pauses, syscalls and blocked goroutines. A production CPU profile saved
  as `default.pgo` in the main package drives PGO (Go 1.21+).

**Rust**

```bash
CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --bin app -- <args>   # perf or dtrace
samply record ./target/release/app <args>                              # Firefox Profiler UI
perf record -F 99 --call-graph dwarf ./target/release/app
perf script | inferno-collapse-perf | inferno-flamegraph > flame.svg
```

Release builds with symbols (`debug = "line-tables-only"` under
`[profile.release]`, or the variable above). Heap: the `dhat` crate or
heaptrack. Async: tokio-console (`console-subscriber`, built with
`RUSTFLAGS="--cfg tokio_unstable"`) shows tasks that poll too long.

**JVM**

```bash
asprof -d 30 -e cpu -f cpu.html <pid>      # async-profiler 3+; also -e wall, alloc, lock
jcmd <pid> JFR.start duration=60s filename=rec.jfr settings=profile
jfr view hot-methods rec.jfr                # JDK 21+, or open rec.jfr in JDK Mission Control
jcmd <pid> Thread.print                     # thread dumps: three, 5s apart, for a hang
```

async-profiler and JFR sample without safepoint bias (thread-dump
samplers blame the wrong lines). JFR costs about 1% (default) to 2%
(`profile`), cheap enough for production. GC detail: `-Xlog:gc*`.

**.NET**

```bash
dotnet-counters monitor -p <pid> --counters System.Runtime   # CPU, GC, allocation, thread pool
dotnet-trace collect -p <pid> --duration 00:00:00:30 --format Speedscope
dotnet-gcdump collect -p <pid>                                # heap by type
```

PerfView (Windows), Visual Studio and dotTrace go deeper. Thread pool
starvation (usually sync over async: `.Result`, `.Wait()`) shows as thread
count and queue length rising while CPU stays low.

**Native code and the whole machine**

```bash
perf stat -d ./app                                   # cycles, IPC, cache and branch misses
perf record -F 99 -g -p <pid> -- sleep 30 && perf report
perf script | stackcollapse-perf.pl | flamegraph.pl > cpu.svg
offcputime-bpfcc -df -p <pid> 30 > off.stacks        # bcc; offcputime on some distributions
flamegraph.pl --color=io --countname=us < off.stacks > off.svg
bpftrace -e 'profile:hz:99 /pid == 1234/ { @[ustack] = count(); }'
```

- In containers and VMs, perf needs `kernel.perf_event_paranoid` lowered
  or `CAP_PERFMON`; hardware counters may be missing.
- macOS: Instruments (Time Profiler, Allocations) or `sample <pid> 10`.
  VTune and AMD uProf add hardware detail; Valgrind's callgrind counts
  every instruction, tens of times slower.
- IPC under about 1 with many cache misses means the loop waits on memory,
  not arithmetic: the fix is data layout (`systems-memory`).

**Databases** profile themselves (`database-queries`):

```sql
-- Postgres, with pg_stat_statements in shared_preload_libraries
SELECT calls, round(total_exec_time) AS total_ms,
       round(mean_exec_time::numeric, 1) AS mean_ms, rows, query
FROM pg_stat_statements ORDER BY total_exec_time DESC LIMIT 20;
```

Rank by total time, not mean: a 2ms query run 50,000 times an hour costs
more than a 3s report run twice. Then `EXPLAIN (ANALYZE, BUFFERS)` on the
top ones. MySQL: the slow query log and `performance_schema` digests.

## 6. Profiling in production, safely

- Sample, never instrument: 99 Hz for 30 to 60 seconds on one instance,
  out of the load balancer when the tool is heavy. Continuous profilers
  (Grafana Pyroscope, Parca with eBPF, Datadog and other APMs) keep a
  low-rate profile of every instance, so a bad hour can be compared with a
  good one afterwards.
- Profiling endpoints (`/debug/pprof`, the Node inspector, JMX) are never
  public: localhost or an internal network behind auth. An open inspector
  port is remote code execution.
- Heap dumps hold whatever was in memory (tokens, passwords, personal
  data): store them like secrets, never attach them to tickets or chats,
  delete them after. They pause the process and can need as much memory
  again as the heap: take them from an instance out of rotation.

## 7. From a profile to a change

Take the top five frames by self time and by total time; for each, find
why it is called (walk down to your code) and how often, then ask in order:

1. **Avoid it?** Results never used, log lines formatted at a disabled
   level, validation repeated at every layer, data fetched just in case.
2. **Do it once?** Hoisted out of the loop, per request instead of per
   item, memoised, cached (`backend-caching`), precomputed on write.
3. **Batch it?** One query or bulk call for N items (`performance-backend`).
4. **Make it cheaper?** A better algorithm or data structure, fewer
   allocations and copies (`performance-memory`), a measured faster library.
5. **Move it?** To a job (`backend-jobs`), a worker thread, or later.
6. **Parallelise it?** Only when the work is independent and cores sit idle.

Usual findings: JSON parsing and serialisation of large payloads; regular
expressions compiled in a loop; strings built by repeated concatenation;
ORM hydration of thousands of objects; logging; reflection; exceptions as
control flow; the same data sorted repeatedly; lock contention; GC driven
by a high allocation rate. Password hashing (argon2, bcrypt) is slow on
purpose: protect it with rate limits, never lower its cost.

Then profile again under the same workload: the plateau is narrower,
nothing new has grown. Keep both profiles with the report.

## Check it

- The profile came from an optimised build under a realistic workload,
  with thousands of samples and resolved symbols (no wall of `[unknown]`),
  and its type matched the symptom (wall-clock or off-CPU when idle).
- The change targets a frame that was wide, a second profile shows it
  narrower, and the end-to-end measurement confirms it (`performance`).

## Avoid

Optimising without a profile; profiling a debug build, an idle process or
a toy input; reading a flame graph's x-axis as time or its colours as heat;
a CPU profile to explain a service that waits; instrumenting profilers for
proportions; tuning a library frame instead of asking why your code calls
it; pprof or inspector ports reachable from the internet; heap dumps shared
or kept; declaring victory without a second profile.
