---
name: backend-stack-laravel
description: "A Laravel backend: API routes and resource controllers, Form Requests, mass assignment, API Resources, policies and Sanctum, exception rendering, Eloquent without N+1, transactions and race-free updates, queues and the scheduler, config, feature tests. Read when building or changing a Laravel app."
---

# Laravel

The backend rules are in the `backend` skill; these are Laravel's own. Keep
the project's version, structure and packages; the notes below assume
Laravel 11 or later (`bootstrap/app.php` for middleware and exceptions).

## 1. Routes and controllers

- API routes in `routes/api.php` (`php artisan install:api` adds it), grouped
  with a prefix and middleware (`auth:sanctum`); resource controllers
  (`Route::apiResource('orders', OrderController::class)`) and route model
  binding.
- Thin controllers: a Form Request in, one call to an action or service
  class (`app/Actions/PayOrder.php`) that holds the rules, an API Resource
  out.

## 2. Input

- A Form Request per write (`StoreOrderRequest`) with `rules()` and
  `authorize()`; the controller uses `$request->validated()`, never
  `$request->all()`.
- Models list `$fillable` explicitly; never `$guarded = []`. Fields a user
  must not set (`role`, `price`, `user_id`) are assigned in code.

## 3. Output

- API Resources (`OrderResource`, `OrderResource::collection`) decide the
  fields; never return a model or `->toArray()` directly (hidden columns and
  future ones leak).
- Lists with `->paginate(25)` or `->cursorPaginate(25)` in the query,
  `with()` for the relations the resource shows.
- Errors in the format of `backend-errors`, rendered in `bootstrap/app.php`
  with `->withExceptions(...)` for requests that expect JSON; `abort(404)` and
  domain exceptions with a `render` method rather than hand-built responses.

## 4. Who may do what

- Policies per model (`OrderPolicy`) and `$this->authorize('refund',
  $order)` or `Gate`, on every action; queries scoped to the user or tenant
  (a global scope, or `$user->orders()`), so another id in the URL gives
  `404`.
- Sanctum: cookie sessions for the app's own frontend (with `statefulApi()`
  and CSRF), personal access tokens with abilities for mobile and other
  clients. Starter kits (Breeze, Fortify) for sign-in, reset and
  verification rather than writing them.

## 5. Eloquent and data

- `Model::preventLazyLoading(! app()->isProduction())` in
  `AppServiceProvider`, so N+1 fails in development; `with()`,
  `withCount()`, `load()` to load relations.
- `DB::transaction(fn () => ...)` around writes that belong together.
  Race-free stock with a conditional update that reports rows changed:
  `Product::whereKey($id)->where('stock', '>=', $qty)->decrement('stock',
  $qty)` returns 0 when there is not enough; `lockForUpdate()` inside the
  transaction when you must read then write.
- Casts for types (`'total' => 'integer'`, enums, `'immutable_datetime'`),
  money as integers, `timestamps` in UTC (`APP_TIMEZONE=UTC`), converted for
  display.
- Migrations from `php artisan make:migration`, with foreign keys
  (`foreignId('order_id')->constrained()->restrictOnDelete()`) and unique
  indexes; factories and seeders for sample data.

## 6. Queues and the scheduler

- Jobs implementing `ShouldQueue` with `$tries`, `backoff()`, `ShouldBeUnique`
  where a duplicate would harm, and `afterCommit` so they never run before
  the transaction; the database driver to start, Redis with Horizon later.
- Scheduled work in `routes/console.php` with `->onOneServer()` and
  `->withoutOverlapping()`; one `schedule:run` cron or `schedule:work`.
- Mail and notifications queued (`ShouldQueue`), with texts through the lang
  files (the `i18n` skill).

## 7. Config and logs

- `env()` only inside `config/*.php`; code reads `config('services.x.key')`,
  and production runs `php artisan config:cache`.
- `Log::withContext(['request_id' => ...])` in a middleware; no request
  bodies with passwords or tokens in the log.

## 8. Tests

Pest or PHPUnit feature tests with `RefreshDatabase`, factories and
`actingAs($user)`; `postJson()` with `assertStatus`, `assertJsonPath` and
`assertJsonValidationErrors`; `Http::fake()`, `Queue::fake()`, `Mail::fake()`
and `$this->travelTo(...)`; the test database of the same kind as
production (`backend-testing`).

## 9. Check

`php artisan test`, `./vendor/bin/pint --test`, Larastan or PHPStan when the
project has it, `php artisan route:list` for the routes, then call them with
`curl`.

## Avoid

`$request->all()` into `create()`; `$guarded = []`; models returned as JSON;
authorisation only in the frontend; lazy loading in a loop; `env()` outside
config; a job that runs before its transaction commits; scheduled commands
that run on every server; rules written in controllers.
