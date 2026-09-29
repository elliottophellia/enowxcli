---
name: systems
description: "Systems programming: reading a low-level codebase, ownership and lifetimes made explicit, invariants written down, errors that fail loudly, concurrency and memory decisions, portability, unsafe code with its proof, measuring before optimising, and the checks (warnings, sanitizers, fuzzing, Miri) that make it done. The principles and which skill holds each. Read before any low-level, performance-critical or unsafe work."
---

# Systems programming

The default version compiles and passes the happy-path test: `unwrap` on
every `Result`, an `unsafe` block with no word on why it is sound, one
mutex around all the state, a fresh `Vec` per iteration of the hot loop,
`len as u32` truncating in silence, a parser that trusts the length field
it just read, "optimised" code nobody measured, and a hard-coded `/tmp`
path. It breaks on the first hostile input, the first contended lock, the
first other platform. This skill is the discipline around the code: what
to read first, the principles, what makes a change done, and which
`systems-*` skill holds the detail.

## 1. Read the project before changing it

- **Toolchain**: `rust-toolchain.toml` (pinned channel, components,
  targets), `edition` and `rust-version` in `Cargo.toml`; for C and C++,
  `CMakePresets.json`, toolchain files, the standard set by
  `target_compile_features` or `CMAKE_CXX_STANDARD`, and the compilers CI
  uses. Work within them: no nightly feature in a stable crate, no C++23
  library call in a C++17 codebase.
- **Targets and features**: `[features]`, `cfg(...)` gates,
  `.cargo/config.toml` (linker, `rustflags`, runners), CMake options. The
  change builds with every feature set CI builds.
- **Where the danger lives**, before you edit near it:

  ```sh
  rg -n 'unsafe (fn|impl|extern|\{)|extern "C"' --type rust
  rg -n 'reinterpret_cast|memcpy|malloc|free\(|\bnew |delete ' --type c --type cpp
  rg -n 'enum \w*Error|thiserror|anyhow' --type rust
  ```

- **Conventions**: the existing error type or return-code scheme, the
  logging macro, the allocator, the test framework. Reuse them; a second
  error type or a second channel crate is a review comment waiting.
- **Baseline**: build and test once before the change and keep the
  output. "No new warnings" and "no new failures" are measured against it.
- **CI**: the workflow files say what passing means (`-D warnings`,
  `-Werror`, sanitizer and Miri jobs, the target matrix). Run the same
  commands locally.

## 2. Principles

- **Invalid states unrepresentable.** An enum instead of two booleans that
  can contradict; `NonZeroU32`, `Port(u16)`, a validated `Utf8Name`
  instead of a raw integer or string every caller re-checks; private
  fields and a constructor that validates.
- **Invariants written where they are relied on.** A comment on the type
  or field ("`len <= cap`; `buf[..len]` is initialised"), a
  `debug_assert!` or `assert()` at the point that depends on it, and a
  test for the edge. An invariant only the author knew is the next
  editor's bug.
- **Errors fail loudly.** Input and environment errors return with context
  (which file, which offset, what was expected). A broken invariant is a
  bug: panic or abort with a message. Never swallow: no
  `unwrap_or_default()` on corrupt data, no ignored return code, no empty
  `catch (...)`.
- **Bounded resources.** Every queue, buffer, cache, recursion and retry
  has a stated limit: a channel of 1024, frames of at most 16 MiB, depth
  64, an LRU of 10 000 entries, three retries with backoff. Unbounded is
  an outage under load.
- **No undefined behaviour.** UB is not "works on my machine": the
  compiler may assume it never happens, and the next version will act on
  that. Overflow is checked or explicitly wrapping; narrowing casts are
  checked.
- **Portability by abstraction.** Platform code in one module
  (`sys/unix.rs`, `sys/windows.rs`, `platform_posix.c`) behind one
  interface; no `#[cfg]` or `#ifdef` threaded through logic; paths built
  with the path API; sizes from `<stdint.h>` types, not `long`. CI tests
  every target you claim; a 32-bit target (`i686-unknown-linux-gnu`)
  catches pointer-size assumptions, a big-endian one under QEMU
  (`cross test --target s390x-unknown-linux-gnu`) catches byte-order bugs.
- **Determinism in tests.** Seeded randomness, an injected clock, no
  `sleep` as synchronisation, output sorted before comparison (hash map
  order is random in Rust and differs between C++ standard libraries).

Which failure gets which treatment:

| Situation | Rust | C++ | C |
|---|---|---|---|
| Bad input, missing file, network or user error | `Result` with context | `std::expected` or an exception, per project policy | a status code, details through an out-parameter or `errno` |
| A broken invariant (a bug) | `assert!` or `unreachable!` with a message | `assert`, and `std::abort()` where going on would corrupt data | `assert`, `abort()` |
| State too damaged to go on | `std::process::abort()` | `std::terminate()` | `abort()` |

- `assert!` stays in release builds: keep it where continuing would
  corrupt memory or data; `debug_assert!` for checks too costly to ship.
- Library code never exits the process or prints; it returns the error
  and the binary decides what to tell the user.

## 3. Ownership and lifetimes, explicit

For every resource (memory, file, socket, lock, thread, mapping) you can
say who owns it, who borrows it, and when it ends.

| Language | Owner | Borrower | Shared |
|---|---|---|---|
| Rust | `T`, `Box<T>`, `Vec<T>` | `&T`, `&mut T`, `&[T]`, `&str` | `Arc<T>`, `Rc<T>` |
| C++ | a value, `std::unique_ptr<T>` | `T&`, `const T&`, `std::span`, `std::string_view` | `std::shared_ptr<T>` |
| C | a documented `foo_create`/`foo_destroy` pair | a pointer documented as borrowed for the call | a count with `_ref`/`_unref` |

- Release on every path by a destructor (`Drop`, RAII) rather than by
  hand; in C, one cleanup label per function.
- Views never outlive the owner: no `string_view` of a temporary, no
  slice kept across a `Vec` that grows, no pointer to a stack buffer
  returned.
- Shared ownership is a decision, not a way to quiet the borrow checker:
  `Arc<Mutex<T>>` everywhere usually means nobody decided who owns the
  state.

## 4. Integers do not overflow in silence

- Rust panics on overflow in debug builds and wraps in release (unless
  `overflow-checks = true` in the profile). Say what you mean:
  `checked_add` for sizes and offsets from input, `saturating_sub` for
  counters that floor at zero, `wrapping_add` for hashes and sequence
  numbers.
- Narrowing with `u32::try_from(len)?`, never `as`; clippy's
  `cast_possible_truncation` and `cast_sign_loss` (pedantic) find the rest.
- C and C++: signed overflow is undefined, unsigned wraps. Use
  `__builtin_add_overflow` and `__builtin_mul_overflow` (GCC, Clang) or C23
  `ckd_add` and `ckd_mul` from `<stdckdint.h>`.
- `count * size` and `offset + len` are the classic heap overflow: checked,
  then compared against what is actually there.

## 5. Unsafe code carries its proof

Unsafe Rust, raw pointers in C++ and all of C are where the compiler
trusts you. Each use gets:

1. **The smallest scope**: one operation per `unsafe` block, inside a
   function whose safe signature makes misuse impossible.
2. **A proof**: a `// SAFETY:` comment naming each requirement of the
   operation (non-null, aligned, initialised, in bounds, no aliasing
   `&mut`, alive long enough) and why it holds here. An `unsafe fn`
   documents its callers' obligations under `# Safety`.
3. **A debug check**: `debug_assert!` of the precondition.
4. **A tool run**: Miri for Rust, ASan and UBSan for C and C++, on tests
   that reach the code.

```rust
/// Bytes `start..end` of the buffer, without a bounds check.
///
/// # Safety
/// `start <= end <= self.len()`.
pub unsafe fn slice_unchecked(&self, start: usize, end: usize) -> &[u8] {
    debug_assert!(start <= end && end <= self.len());
    // SAFETY: the caller guarantees the range is in bounds, and `self.ptr`
    // points to `self.len` initialised bytes (invariant of `Buffer`).
    unsafe { std::slice::from_raw_parts(self.ptr.add(start), end - start) }
}
```

Enforce it with `unsafe_op_in_unsafe_fn` (warns by default in edition
2024) and clippy's `undocumented_unsafe_blocks` and `missing_safety_doc`.
If the proof cannot be written, the code is not sound: use the safe
version (usually a few percent slower, and fine) or restructure.

## 6. Concurrency and memory are decisions

Write them down before the code, in the module doc or the change
description:

- Which threads or tasks exist, what each owns, how they talk (a channel,
  or shared state behind which lock), what happens when a consumer is
  slower than its producer (block, drop, reject), and how everything stops
  (`systems-concurrency`).
- Where the data lives, how long, who frees it, and the expected peak
  (`systems-memory`). A buffer sized from input is capped.

## 7. Measure before optimising

1. Fix the workload: real input at sizes that matter, a release build
   with symbols (`debug = "line-tables-only"`, or `-O2 -g`).
2. Baseline: `hyperfine --warmup 3 'old/app input' 'new/app input'` for a
   command; criterion or Google Benchmark for a function; peak RSS from
   `/usr/bin/time -v` (Linux) or `/usr/bin/time -l` (macOS).
3. Profile to find the hot path: `perf record --call-graph dwarf` then
   `perf report`, `samply record` (Linux and macOS), Instruments,
   `cargo flamegraph`.
4. Change one thing where the profile points; measure again with the same
   command, enough runs to see the noise.
5. Keep it only if the gain is real on the real workload. Under about 5%
   off the bottleneck, keep the simpler code. Record the trade-off in the
   commit or a comment: what got faster, for which input, at what cost.

Detail in `performance-profiling` and `performance-benchmarks`.

## 8. Done means

| Check | Rust | C and C++ |
|---|---|---|
| Builds on every supported target, no new warnings | `RUSTFLAGS="-D warnings" cargo build --all-targets --locked` | `-Wall -Wextra -Wpedantic -Werror` (MSVC `/W4 /WX`) |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` | `clang-tidy -p build` on touched files |
| Format | `cargo fmt --all --check` | `clang-format --dry-run --Werror` |
| Tests, one for the edge case handled | `cargo test` or `cargo nextest run` | `ctest --test-dir build --output-on-failure` |
| Unsafe or pointer code changed | `cargo +nightly miri test` | ASan and UBSan build of the tests |
| Threads changed | loom model, TSan, stress loop | TSan build, stress loop |
| Parser or decoder changed | `cargo +nightly fuzz run <target>` for minutes | libFuzzer or AFL++ run |
| Performance claim | before and after numbers, same benchmark | same |

The edge-case test fails without the change and passes with it.

## 9. Which skill holds what

| Job | Skill |
|---|---|
| Principles, done, this map | `systems` |
| Rust idioms, errors, traits, unsafe, tokio, tooling, MSRV, cross builds | `systems-rust` |
| C and C++: RAII, UB, CMake, warnings, sanitizers, fuzzing, ABI | `systems-c-cpp` |
| Threads, locks, atomics, async cancellation, backpressure, loom, TSan | `systems-concurrency` |
| Allocation, arenas, leaks, layout, cache locality, mmap, RSS, GC knobs | `systems-memory` |
| Calling across languages: C ABI, bindgen, cbindgen, cxx, PyO3, napi-rs, wasm | `systems-ffi` |
| Binary formats, versioning, untrusted input, checksums, compression | `systems-formats` |
| Files, fsync, processes, signals, stdio, paths, time, sockets, CLIs | `systems-os` |
| Finding where the time goes | `performance-profiling` |
| Benchmarks that can be trusted | `performance-benchmarks` |

## Check it

- Run the section 8 table with `bash` and read the output, not only the
  exit code: count warnings before and after
  (`cargo build 2>&1 | grep -c '^warning'`).
- `git diff | grep -nE 'unwrap\(\)|expect\(|as (u|i)(8|16|32)|unsafe|todo!|dbg!|printf\('`:
  every hit left is justified by a comment or an invariant proven nearby.
- Every limit you added has a test at the limit and one past it.
- Say plainly what could not be run (no nightly for Miri, no Linux for
  TSan or perf, no Windows machine) and what that leaves unverified.

## Avoid

`unwrap` on anything input or the environment can cause; `unsafe` without
a `SAFETY` proof; a lock held across I/O or an `.await`; queues, buffers,
caches or recursion without a limit; `as` casts that truncate; lengths
from input used before they are checked against what remains; optimising
without a profile, or claiming a speed-up without numbers; platform paths,
separators and integer sizes hard-coded; warnings left for later; calling
a change done because it compiled.
