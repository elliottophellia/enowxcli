---
name: backend-stack-go
description: "A Go backend: net/http routing (or chi), handlers and services, context and timeouts, wrapped errors mapped in one place, sqlc or pgx, slog, graceful shutdown, table-driven tests with httptest. Read when building or changing a Go backend."
---

# Go

The backend rules are in the `backend` skill; these are Go's own.

## 1. Setup

- A module per service (`go.mod` with the Go version), the standard layout
  the project uses (`cmd/api/main.go`, `internal/<feature>/`), `gofmt` and
  `go vet` clean, golangci-lint when the project has it.
- Routing with `net/http` (Go 1.22+ patterns: `mux.HandleFunc("GET
  /orders/{id}", h.get)`, `r.PathValue("id")`); chi when the project uses it.
  No framework added for a handful of routes.

## 2. Structure

- `main` builds the config, the pool, the services and the handlers, and
  passes them in: no package-level globals for the database or the logger.
- Per feature: a handler type that decodes, validates, calls the service
  and encodes; a service with the rules; a store with the queries; the types
  in the package.
- Validation in a method on the request type (`func (r CreateOrder) Valid()
  map[string]string`) or go-playground/validator, then the handler answers
  `400` with the field errors.

## 3. Context and time limits

- Every handler passes `r.Context()` down to the service, the store and
  outbound calls, so a client that goes away cancels the work.
- Timeouts: `http.Server` with `ReadHeaderTimeout`, `ReadTimeout`,
  `WriteTimeout` and `IdleTimeout` set; `http.MaxBytesReader` on bodies;
  `context.WithTimeout` around slow calls; an `http.Client` with a `Timeout`,
  never `http.DefaultClient`.

## 4. Errors

- Errors are values: wrapped with context (`fmt.Errorf("pay order %d: %w",
  id, err)`), compared with `errors.Is` and `errors.As`, never by their
  string.
- Domain errors as sentinel values or types (`ErrNotFound`, `*ConflictError`)
  mapped to status codes in one helper that every handler calls to write
  errors, in the format of `backend-errors`.
- No `panic` for expected failures; a recovery middleware turns an
  unexpected panic into a `500` and a log line.

## 5. Data and logs

- sqlc (SQL written by hand, Go generated from it) or pgx directly; migrations
  with goose, atlas or golang-migrate; transactions with `pool.BeginTx` and a
  deferred `Rollback` that is a no-op after `Commit`.
- `log/slog` with a JSON handler, the request id added by middleware and
  carried in the context.

## 6. Shutdown

```go
srv := &http.Server{Addr: cfg.Addr, Handler: mux, ReadHeaderTimeout: 5 * time.Second}
go func() {
    if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
        logger.Error("server", "err", err)
        os.Exit(1)
    }
}()
ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
defer stop()
<-ctx.Done()
shutdownCtx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
defer cancel()
if err := srv.Shutdown(shutdownCtx); err != nil {
    logger.Error("shutdown", "err", err)
}
pool.Close()
```

## 7. Tests

Table-driven tests; `httptest.NewRecorder` with the handler, or
`httptest.NewServer` for the whole router; a real database per
`backend-testing` (testcontainers-go, or a test schema); fakes behind small
interfaces for other services; `go test -race ./...`.

## 8. Check

`go build ./...`, `go vet ./...`, `go test -race ./...`, then run it and call
the endpoints with `curl`.

## Avoid

Globals for the database or config; `http.DefaultClient` or a server with no
timeouts; ignored errors (`_ =` on anything that can fail); errors compared by
string; `panic` for a bad request; a goroutine per request left running;
`context.Background()` inside a handler.
