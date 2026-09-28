---
name: backend-stack-rust
description: "A Rust backend: axum on tokio, extractors and shared state, serde validation, thiserror errors turned into responses, sqlx with migrations, tracing, tower-http layers, graceful shutdown, tests with oneshot. Read when building or changing a Rust backend."
---

# Rust

The backend rules are in the `backend` skill; these are Rust's own.

## 1. Setup

- axum on tokio (or the project's framework), `serde` for bodies, `sqlx` for
  the database, `tracing` for logs, `tower-http` for the layers around the
  router. `cargo fmt` and `cargo clippy -- -D warnings` clean.
- Layout: `main.rs` builds config, pool and router; a module per feature
  (`orders/{mod,routes,service,store}.rs`); `error.rs`, `config.rs`,
  `state.rs` at the top.

## 2. Handlers and state

- Shared state in one `AppState` (the pool, config, clients) behind `Clone`
  (an `Arc` inside where needed), passed with `.with_state(state)` and taken
  with `State(state)`; no `static` or `lazy_static` for the database.
- Extractors do the parsing: `Path<OrderId>`, `Query<ListParams>`,
  `Json<CreateOrder>`. A custom extractor (`CurrentUser`) loads and checks
  the caller once, for every route that takes it.
- Bodies with `#[serde(deny_unknown_fields)]` on inputs, then validated
  (the `validator` crate or a `fn validate(&self) -> Result<(), FieldErrors>`)
  before the service is called. Newtypes for ids and money
  (`struct OrderId(i64)`, `struct Amount(i64)`), so they cannot be mixed up.

## 3. Errors

- One `AppError` enum with `thiserror` (`NotFound`, `Forbidden`,
  `Conflict(&'static str)`, `Validation(FieldErrors)`, `Internal(#[from]
  anyhow::Error)`), and `impl IntoResponse for AppError` that writes the
  format in `backend-errors` and logs internal errors with their chain.
- Handlers return `Result<Json<T>, AppError>` and use `?`. No `unwrap` or
  `expect` on anything a request can cause.

## 4. Data

- `sqlx` with `query!` or `query_as!` checked at compile time (with
  `cargo sqlx prepare` for offline builds), migrations in `migrations/` run
  by `sqlx migrate run`, transactions with `pool.begin()` and `tx.commit()`.
- `PgPool` created once with limits (`max_connections`, `acquire_timeout`).

## 5. Around the router

- `tower-http` layers: `TraceLayer` with a request id
  (`SetRequestIdLayer`, `PropagateRequestIdLayer`), `TimeoutLayer`,
  `RequestBodyLimitLayer`, `CorsLayer` with explicit origins,
  `CompressionLayer`.
- `tracing-subscriber` with JSON output in production and `EnvFilter` from
  `RUST_LOG`.

## 6. Shutdown

```rust
let listener = tokio::net::TcpListener::bind(&config.addr).await?;
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
pool.close().await;
```

with `shutdown_signal` awaiting `ctrl_c` or `SIGTERM`.

## 7. Tests

Build the router with a test state and call it with
`tower::ServiceExt::oneshot(request)`; `#[sqlx::test]` gives each test a
fresh database with the migrations applied; wiremock for outbound HTTP.

## 8. Check

`cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
then run it and call the endpoints with `curl`.

## Avoid

`unwrap` in handlers; a global pool; `String` for every id; errors turned
into responses in each handler; bodies without `deny_unknown_fields` or
validation; no body limit or timeout layer; blocking calls on the async
runtime without `spawn_blocking`.
