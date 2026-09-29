---
name: performance-benchmarks
description: "Benchmarks that can be trusted: micro versus macro, warm-up and many iterations, controlling noise (CPU frequency, background load, isolation), realistic inputs, defeating dead-code elimination, comparing runs with statistics, the tools for each language (criterion, hyperfine, tinybench, pytest-benchmark, JMH, BenchmarkDotNet, Go testing.B with benchstat), and catching regressions in CI. Read before writing a benchmark or claiming a speed-up."
---

# Benchmarks that can be trusted

A generated benchmark wraps one call in `console.time`, runs it once on a
laptop on battery, and reports "3x faster". Or it loops over a result it
never uses, so the compiler deletes the work and the benchmark times an
empty loop; or it compares two runs 5% apart when the noise is 10%. This
skill is the method, the tools per language and the CI setup that make a
speed-up claim hold. The wider method (baseline, workload, reporting) is in
`performance`.

## 1. Micro, macro, and what each tells

| Kind | Measures | Answers |
|---|---|---|
| Micro | one function in a tight loop | which of two implementations of a hot spot is faster, for this input, on this machine |
| Macro | a whole command, request or job | what a user or an operator will feel |

- A micro-benchmark isolates the code from what decides real performance:
  cold caches, other data competing for the cache, GC caused by the rest
  of the program, contention, I/O. It can say A beats B; it cannot say the
  product got faster.
- Use micro to choose between implementations of a hot spot the profile
  found (`performance-profiling`), then confirm with a macro measurement:
  hyperfine, a timed job on real data, a load test (`performance-load`).
  A micro win that does not show end to end is not a win for users.

## 2. Method

- **Warm up** to a steady state: JIT compiled, caches filled, lazy
  initialisation done, clocks settled. The tools below all do this;
  hand-rolled loops usually do not.
- **Many samples.** Let the tool decide: criterion takes 100 samples over
  about 5s after 3s of warm-up; hyperfine runs at least 10 times and at
  least 3s. By hand, at least 10 runs of a macro measurement.
- **Median and spread** (interquartile range, median absolute deviation or
  standard deviation) with the sample count. A mean alone is dragged by
  outliers. The minimum is a fair summary only for deterministic CPU-bound
  micro-benchmarks, where noise can only add time.
- **A statistical comparison**, not a glance: benchstat (Mann-Whitney U,
  `~` when no significant difference), criterion's change report against a
  saved baseline (p < 0.05 and a 1% noise threshold by default), pyperf
  `compare_to`, BenchmarkDotNet's statistical test column. What the test
  cannot tell from noise is reported as no measurable difference.
- **Stability**: run the whole suite two or three times. If the medians
  move more than the effect, fix the noise before believing anything.
- **Interleave** before and after (A B A B) in macro comparisons, so
  drift from heat or background jobs lands on both.

## 3. Control the noise

- Same machine, same session, plugged in, nothing else running (browsers,
  IDE indexers, backups, other benchmarks). Laptops throttle on battery and
  when hot: let them cool between long runs.
- Fix the CPU frequency on Linux: `sudo cpupower frequency-set -g
  performance`, and turn off turbo boost (Intel:
  `echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo`; with
  cpufreq boost: `echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost`).
  `python -m pyperf system tune` does this and more; `system reset` undoes
  it.
- Pin the process: `taskset -c 2,3 ./bench`; for serious work, keep those
  cores away from the scheduler (`isolcpus`).
- macOS offers none of these controls: stay plugged in with Low Power Mode
  off, and trust only relative comparisons made in one session.
- Disk cache: measure warm (after a warm-up run) or cold on purpose
  (Linux `sync; echo 3 | sudo tee /proc/sys/vm/drop_caches`, macOS
  `sudo purge`, in hyperfine's `--prepare`), and say which.
- Cloud CI runners are shared and commonly vary by 10% or more between
  runs, sometimes even in CPU model. Compare base and head in the same job
  on the same runner, count instructions instead of time (Valgrind-based
  tools, CodSpeed's CPU simulation) for stable CPU-bound results, or use a
  dedicated runner.

## 4. Realistic inputs

- Sizes and distributions from production: real string lengths and item
  counts, key skew (a few hot keys), the share of hits and misses, sorted
  versus random order (sorted input trains the branch predictor and
  flatters the code).
- Several sizes (10, 1,000, 100,000, 10 million) to see how cost scales.
  An O(n^2) path looks fine at 100 and dominates at 100,000; a simpler
  algorithm can win below a crossover size, so find where it is.
- The worst cases: the largest customer, pathological input, many
  duplicates, empty input.
- Generated data from a fixed seed, so every run sees the same input.
- Real dependencies where they are the cost: a mocked database removes
  exactly what an endpoint benchmark was meant to measure.

## 5. Pitfalls that fake a result

| Pitfall | Symptom | Fix |
|---|---|---|
| Dead-code elimination: the result is unused, so the compiler removes the work | impossibly fast (nanoseconds for real work), flat across input sizes | consume the result: `std::hint::black_box`, JMH `Blackhole` or a returned value, Go `b.Loop()` or a package-level sink, mitata `do_not_optimize`, a value returned from a BenchmarkDotNet method |
| Constant folding: inputs known at compile time | same as above | read inputs from state or through `black_box`, never literals |
| Setup inside the timed region | cost follows the setup, not the code | setup outside: criterion `iter_batched`, JMH `@Setup`, Go `b.Loop()` or `b.ResetTimer()`, pytest-benchmark `pedantic(setup=...)` |
| JIT not warmed, or profiles polluted by other benchmarks | bimodal or drifting results | warm-up iterations; JMH forks (`@Fork`) run each benchmark in a fresh JVM |
| GC during measurement | high variance | report allocations (`-benchmem`, `[MemoryDiagnoser]`, JMH `-prof gc`) and run long enough to average GC in |
| Hot caches: the same small input every iteration | faster than reality | rotate through inputs larger than the CPU cache (L3 is tens of MB) |
| Timer resolution | zeros, or identical values | many iterations per sample (the tools do this); browsers coarsen `performance.now()` to between 5 microseconds and 1 millisecond |
| A debug build | slow in a way nobody ships | release or production builds; `cargo bench` already uses an optimised profile |
| Different machines, versions or data before and after | a difference that is not the change | one machine, one session, versions recorded |

## 6. The tools

| Language | Tool | Notes |
|---|---|---|
| Rust | criterion, divan | criterion: statistics, HTML reports, `--save-baseline` and `--baseline`; divan: lighter, attribute-based, counts allocations with `AllocProfiler` |
| Any command | hyperfine | `--warmup`, `--prepare`, `-N` for fast commands without a shell, `-P` to scan a parameter, `--export-markdown` |
| JavaScript | tinybench, mitata, Vitest `bench` | `vitest bench` runs on tinybench; Deno has `deno bench` |
| Python | pytest-benchmark, pyperf | pytest-benchmark saves and compares runs; pyperf runs worker processes and tunes the system |
| JVM | JMH | the only trustworthy harness on the JVM; Gradle plugin `me.champeau.jmh` or the Maven archetype |
| .NET | BenchmarkDotNet | Release builds only; `[MemoryDiagnoser]`, `[Params]`, a `Baseline = true` method |
| Go | `testing.B` with benchstat | `-benchmem`, `-count=10`, `b.Loop()` (Go 1.24+) |

Go, compared with benchstat:

```go
func BenchmarkParseOrders(b *testing.B) {
	data := loadFixture(b, "testdata/orders-10k.json") // setup: not timed with b.Loop
	b.ReportAllocs()
	for b.Loop() {
		if _, err := ParseOrders(data); err != nil {
			b.Fatal(err)
		}
	}
}
```

```bash
go test -run='^$' -bench=ParseOrders -benchmem -count=10 ./orders > old.txt
# apply the change, then the same command > new.txt
benchstat old.txt new.txt        # go install golang.org/x/perf/cmd/benchstat@latest
```

Rust with criterion (`[[bench]] name = "parse"` and `harness = false` in
`Cargo.toml`), across sizes:

```rust
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_orders");
    for n in [100, 10_000, 1_000_000] {
        let input = fixtures::orders_json(n); // fixed seed inside
        group.bench_with_input(BenchmarkId::from_parameter(n), &input, |b, input| {
            b.iter(|| parse_orders(black_box(input)))
        });
    }
    group.finish();
}
criterion_group!(benches, bench_parse);
criterion_main!(benches);
```

JVM with JMH (run with `./gradlew jmh`, or `java -jar target/benchmarks.jar`
from the Maven archetype):

```java
@State(Scope.Benchmark)
@BenchmarkMode(Mode.AverageTime) @OutputTimeUnit(TimeUnit.MICROSECONDS)
@Warmup(iterations = 5, time = 1) @Measurement(iterations = 10, time = 1) @Fork(3)
public class ParseOrdersBench {
    @Param({"100", "10000"}) public int size;
    private String json;
    @Setup public void setup() { json = Fixtures.ordersJson(size); }
    @Benchmark public List<Order> parse() { return Parser.parseOrders(json); } // returned: not eliminated
}
```

.NET with BenchmarkDotNet (`BenchmarkRunner.Run<ParseOrdersBench>()`, then
`dotnet run -c Release`):

```csharp
[MemoryDiagnoser]
public class ParseOrdersBench
{
    [Params(100, 10_000)] public int Size;
    private string _json = "";
    [GlobalSetup] public void Setup() => _json = Fixtures.OrdersJson(Size);
    [Benchmark(Baseline = true)] public List<Order> Current() => Parser.Parse(_json);
    [Benchmark] public List<Order> Streaming() => Parser.ParseStreaming(_json);
}
```

JavaScript with mitata:

```js
import { bench, run, do_not_optimize } from "mitata";

const input = makeOrders(10_000, { seed: 42 });
bench("parse, current", () => do_not_optimize(parseOrders(input)));
bench("parse, streaming", () => do_not_optimize(parseOrdersStreaming(input)));
await run();
```

When the work has a size, report throughput so sizes compare: criterion
`group.throughput(Throughput::Bytes(n))` or `Elements`, Go `b.SetBytes(n)`
(prints MB/s). Contention only shows with several threads: Go
`b.RunParallel`, JMH `@Threads`.

A command before and after, and Python with pytest-benchmark:

```bash
hyperfine --warmup 3 --runs 20 --export-markdown bench.md \
  './target/release/tool-old data/big.csv' './target/release/tool-new data/big.csv'
pytest --benchmark-only --benchmark-autosave                 # on the base commit
pytest --benchmark-only --benchmark-compare --benchmark-compare-fail=median:5%
```

```python
def test_parse_orders(benchmark, orders_10k):
    result = benchmark(parse_orders, orders_10k)
    assert len(result) == 10_000  # a benchmark that checks nothing may time nothing
```

## 7. Regressions in CI

- Benchmark the hot paths the profiles found, not everything: a suite that
  takes 40 minutes stops being run. A handful to a few dozen benchmarks
  finishing in minutes is the right size.
- Compare relatively, in one job: run the base, run the head, compare with
  the tool's statistics. Absolute numbers from different runners are
  noise.
- Tools: CodSpeed (instrumented measurement that holds steady on shared
  runners; plugs into criterion, divan, pytest, Vitest and others),
  Bencher (`bencher run`, history, statistical thresholds, self-hostable),
  github-action-benchmark (`alert-threshold`, `fail-on-alert`,
  `comment-on-alert`), or a script around benchstat, criterion baselines or
  `--benchmark-compare-fail`.
- Thresholds above the measured noise: 10 to 20% for wall-clock time on
  shared runners, a few percent with instruction counts or a dedicated
  machine. A threshold inside the noise fails at random and gets ignored.
- Deterministic budgets are cheaper than timings and never flaky: binary
  size, bundle size (size-limit), allocations per operation, queries per
  request.
- An accepted regression (a trade for correctness or security) updates the
  baseline explicitly, with the reason in the commit.

## 8. Report it

```text
Benchmark:  parse_orders, 10k orders (fixture, fixed seed), criterion, 100 samples
Machine:    <CPU model, cores, RAM>, <OS and kernel>, governor and turbo settings
Toolchain:  <compiler or runtime version, flags>
Commits:    before <sha>, after <sha>
Before:     median <n> (IQR <n> to <n>)
After:      median <n> (IQR <n> to <n>)
Change:     <n>% faster, p = <n> (or: no measurable difference)
Also:       allocations per op <before> to <after>; end to end <macro result>
```

- Include the exact commands and the raw tool output (benchstat table,
  hyperfine markdown), not a retyped summary.
- Say what the benchmark does not cover: other sizes, other machines,
  concurrency, cold start.

## Check it

- The result scales with input size as expected; a benchmark that takes
  the same time for 100 and 100,000 items was optimised away.
- Two separate runs agree within the spread, and the difference passes the
  tool's significance test.
- The benchmark asserts or consumes its result, and its setup sits outside
  the timed region.
- A macro measurement confirms the micro win, and the tests pass.

## Avoid

`console.time` or `time.time()` around one call; one run; a debug build;
results the compiler can delete; setup inside the timed loop; one tiny
input; numbers compared across machines or CI runners; a difference
inside the noise reported as a win; a mean without the spread; a
micro-benchmark claimed as a product speed-up; thresholds at zero;
benchmarks nobody runs.
