---
name: rs-lazy-static
description: `lazy_static!` or `once_cell` where the standard library now has it
severity: remind
match: lazy_static!
match: once_cell::(sync|unsync)::Lazy
files: *.rs
---

Since Rust 1.80 the standard library has `std::sync::LazyLock` and
`std::cell::LazyCell` (and `OnceLock` for set-once values):

```rust
static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
```

Keep the crate only when the project's minimum Rust version is older.
