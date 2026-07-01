---
name: loom-golang
description: Go language expertise for idiomatic, production-quality code. Use for Go development, concurrency patterns with goroutines and channels, error handling, testing, and module management following Effective Go principles.
triggers:
  - go
  - golang
  - goroutine
  - channel
  - interface
  - struct
  - pointer
  - slice
  - map
  - defer
  - context
  - error
  - gin
  - echo
  - fiber
  - cobra
  - viper
  - gorm
  - sqlx
  - go mod
  - go test
  - effective go
  - errgroup
  - sync
  - mutex
  - waitgroup
  - race detector
---

# Go Language Expertise

## Overview

Idiomatic, production-grade Go: concurrency, error handling, interfaces, testing, and version-gated behavior. The Foundations below get code working; the **Expert Practices** section is the higher bar — each item states the *mechanism* so you can apply it beyond the case shown.

## Foundations

### Error Handling

```go
// Sentinel errors for identity checks; custom types for structured context.
var ErrNotFound = errors.New("resource not found")

type ValidationError struct{ Field, Message string }

func (e *ValidationError) Error() string {
    return fmt.Sprintf("validation on %s: %s", e.Field, e.Message)
}

func fetchUser(id string) (*User, error) {
    u, err := db.GetUser(id)
    if err != nil {
        if errors.Is(err, sql.ErrNoRows) {
            return nil, fmt.Errorf("user %s: %w", id, ErrNotFound) // wrap w/ %w
        }
        return nil, fmt.Errorf("fetching user %s: %w", id, err)
    }
    return u, nil
}

// errors.Is matches by identity down the %w chain; errors.As extracts a type.
var ve *ValidationError
if errors.As(err, &ve) { /* ve.Field, ve.Message */ }
```

`%w` vs `%v` is an API decision — see Expert Practices.

### Concurrency

```go
// Worker pool. Go 1.25+: wg.Go does Add(1)+launch+Done atomically (see Expert).
func workerPool(jobs <-chan Job, results chan<- Result, n int) {
    var wg sync.WaitGroup
    for range n {
        wg.Go(func() { // pre-1.25: wg.Add(1); go func(){ defer wg.Done(); ... }()
            for job := range jobs {
                results <- process(job)
            }
        })
    }
    wg.Wait()
    close(results) // sender closes, exactly once; never the receiver
}

// Context timeout — always defer cancel() even when the deadline fires.
func fetch(ctx context.Context, url string) ([]byte, error) {
    ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
    defer cancel()
    req, _ := http.NewRequestWithContext(ctx, "GET", url, nil)
    resp, err := http.DefaultClient.Do(req)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()
    return io.ReadAll(resp.Body) // drain AND close — see Expert (conn reuse)
}

// select with nil-channel disable: a nil channel blocks forever, so setting a
// drained channel to nil removes it from the select without a sentinel flag.
select {
case v, ok := <-ch:
    if !ok { ch = nil; continue }
    use(v)
case <-ctx.Done():
    return ctx.Err()
}

// RWMutex-guarded state. Pointer receiver is mandatory (copying a Mutex breaks it).
type SafeCounter struct {
    mu    sync.RWMutex
    count map[string]int
}
func (c *SafeCounter) Inc(k string) { c.mu.Lock(); defer c.mu.Unlock(); c.count[k]++ }
func (c *SafeCounter) Get(k string) int { c.mu.RLock(); defer c.mu.RUnlock(); return c.count[k] }
```

**Bounded concurrency with `errgroup`** (preferred over hand-rolled fan-out): first non-nil error cancels the group's context and is returned by `Wait`; `g.SetLimit(n)` caps concurrent goroutines.

```go
g, ctx := errgroup.WithContext(ctx)
g.SetLimit(8)
results := make([]Result, len(urls))
for i, url := range urls { // Go 1.22+: no `i, url := i, url` needed
    g.Go(func() error {
        r, err := fetchURL(ctx, url)
        results[i] = r // distinct index per goroutine => no lock needed
        return err
    })
}
if err := g.Wait(); err != nil { return nil, err }
```

**Mutex vs channel:** use a `Mutex` to guard simple shared state (counters, caches, maps); use a channel to transfer *ownership* of data or coordinate goroutine lifecycles/pipelines. Don't reach for a channel where a mutex is plainly simpler.

### Interfaces & Embedding

```go
// Small interfaces; accept interfaces, return concrete types (see Expert).
type UserGetter interface {
    GetUser(ctx context.Context, id string) (*User, error)
}

// Embedding for composition (promotes Base's fields/methods onto User).
type Base struct{ ID string; CreatedAt, UpdatedAt time.Time }
type User struct {
    Base
    Email, Name string
}
```

### Functional Options

Variadic `Option func(*T)` closures over a zero-value-defaulted struct — the idiomatic way to give a constructor optional, backward-compatible parameters.

```go
type Server struct{ host string; port int; timeout time.Duration }
type Option func(*Server)

func WithPort(p int) Option { return func(s *Server) { s.port = p } }

func NewServer(opts ...Option) *Server {
    s := &Server{host: "localhost", port: 8080, timeout: 30 * time.Second}
    for _, opt := range opts {
        opt(s)
    }
    return s
}
// NewServer(WithPort(9000))
```

### HTTP Servers

```go
srv := &http.Server{
    Addr: addr, Handler: mux,
    ReadTimeout: 15 * time.Second, WriteTimeout: 15 * time.Second, IdleTimeout: 60 * time.Second,
}
// ListenAndServe returns ErrServerClosed on graceful Shutdown — treat as non-error.
if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) { ... }

// Go 1.22+ method+wildcard routing in the stdlib mux — no framework needed for basics.
mux.HandleFunc("GET /api/users/{id}", h.GetUser)   // id := r.PathValue("id")
mux.HandleFunc("POST /api/users", h.CreateUser)

// Recovery middleware — a panic in a handler otherwise kills the whole server.
func recovery(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        defer func() {
            if err := recover(); err != nil {
                slog.Error("panic", slog.Any("err", err), slog.String("path", r.URL.Path))
                http.Error(w, "internal server error", http.StatusInternalServerError)
            }
        }()
        next.ServeHTTP(w, r)
    })
}
```

Frameworks (`gin`, `echo`, `fiber`) add routing/binding sugar over this; the timeout, graceful-shutdown, and recovery concerns are identical.

### Database Access (sqlx)

```go
type User struct {
    ID    string `db:"id"`
    Email string `db:"email"`
}

func (r *Repo) GetByID(ctx context.Context, id string) (*User, error) {
    var u User
    err := r.db.GetContext(ctx, &u, `SELECT id, email FROM users WHERE id=$1`, id)
    if errors.Is(err, sql.ErrNoRows) {
        return nil, ErrNotFound
    }
    return &u, err
}

// Transaction helper: rollback on error (report both errors), commit on success.
func (r *Repo) WithTx(ctx context.Context, fn func(*sqlx.Tx) error) error {
    tx, err := r.db.BeginTxx(ctx, nil)
    if err != nil {
        return fmt.Errorf("begin tx: %w", err)
    }
    if err := fn(tx); err != nil {
        if rb := tx.Rollback(); rb != nil {
            return fmt.Errorf("rollback: %v (original: %w)", rb, err)
        }
        return err
    }
    return tx.Commit()
}
```

Always use the `...Context` variants (`GetContext`, `SelectContext`, `NamedExecContext`) so queries honor cancellation.

### Structured Logging (slog)

```go
logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
logger.Info("request", slog.String("method", r.Method), slog.Duration("latency", time.Since(start)))
logger = logger.With(slog.String("request_id", rid)) // derive a scoped child logger
```

Log errors with `slog.Any("err", err)`, never `slog.String(..., err.Error())` — see Expert. Hot paths: `slog.LogAttrs` avoids allocation.

### Testing

```go
func TestAdd(t *testing.T) {
    tests := []struct {
        name    string
        a, b    int
        want    int
    }{
        {"positive", 2, 3, 5},
        {"mixed", -1, 5, 4},
    }
    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            t.Parallel() // subtests run concurrently; Go 1.22+ needs no `tt := tt`
            if got := Add(tt.a, tt.b); got != tt.want {
                t.Errorf("Add(%d,%d)=%d want %d", tt.a, tt.b, got, tt.want)
            }
        })
    }
}
```

Run `go test -race ./...` in CI — the race detector (~10x slowdown) is the primary tool for the memory-model gotchas below, but only reports races actually *exercised* at runtime, so races need test coverage to surface. In `go < 1.22` modules the `tt := tt` capture workaround is required; `go fix ./...` removes it after a 1.22 bump.

### Benchmarks

```go
// Go 1.24+: for b.Loop() — setup runs once per -count (not b.N times), and the
// runtime keeps params/results alive so the compiler can't elide the body.
func BenchmarkSort(b *testing.B) {
    data := generateData(1000) // runs once
    for b.Loop() {
        Sort(data)
    }
}
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

### Design

**Define interfaces in the consumer; return concrete types from the producer.** Go interfaces are satisfied implicitly, so a producer never needs to declare the interface its types satisfy. Per Go Code Review Comments: interfaces "belong in the package that uses values of the interface type, not the package that implements those values," and "do not define interfaces before they are used." A broad producer-side interface (created for mocking) couples every consumer to that shape instead of the minimal method set it uses. Idiomatic: the producer returns a concrete type (often `*T`, frequently an unexported struct behind an exported `func New() *myType`) so new methods can be added without breaking callers; each consumer declares the small interface it needs. **Returning an interface is a contract, not style:** it forces the producer's whole method set onto every consumer and blocks additive change — a new method on the concrete type is unusable without widening the interface (a breaking change).

```go
// consumer/service.go — declares exactly what it uses
type UserGetter interface {
    GetUser(ctx context.Context, id string) (*User, error)
}
func NewService(g UserGetter) *Service { /* ... */ }

// producer/repo.go — returns a concrete type, idiomatic even when unexported
func NewUserRepository(db *sqlx.DB) *userRepository { /* ... */ }
```

**Prefer synchronous functions; let callers add concurrency.** Hiding goroutine creation forces concurrency on every caller — they lose control over lifetime, can't add timeouts, can't call synchronously, and testing is harder. The asymmetry is the point: making a sync function async is one line at the call site (`go func(){ errs <- Process(ctx, item) }()`); un-async-ing an async API requires rewriting every caller.

**Design types so the zero value is usable.** Every Go variable is zero-initialized; exploit it so a type needs no constructor and resists init-order bugs. `sync.Mutex` (zero = unlocked), `bytes.Buffer`, `sync.WaitGroup` all work at zero value. Corollaries: name bool fields so `false` is the safe default (prefer `disabled` over `enabled`); add a constructor only when init is genuinely non-trivial. Anti-patterns: requiring an `Init()` before use, or panicking on a zero value.

**`%w` makes the wrapped error part of your public API; use `%v` at boundaries.** `fmt.Errorf("...: %w", err)` lets callers `errors.Is`/`errors.As` the wrapped value, making it part of your contract. Wrap an internal sentinel like `sql.ErrNoRows` with `%w` and callers can depend on it — swapping your DB driver becomes a breaking change. Within an application, or when the sentinel is genuinely meant to be inspectable, use `%w`; at package/system boundaries (RPC, storage, external services) use `%v` to flatten to a string, or convert to your own exported sentinel first. Caveat: `%v` destroys chain identity — a later `errors.Is` against the original returns `false` with no compile error.

**Use generics for type-identical code; use an interface when you only call methods.** Reach for type parameters when you'd otherwise write the same code differing only by concrete type — containers and functions over slices/maps/channels of any element type. But (Go team's "When To Use Generics") "if all you need to do with a value is call a method on it, use an interface type, not a type parameter." `func ReadSome[T io.Reader](r T)` is strictly worse than `func ReadSome(r io.Reader)` — same speed, harder to read, no benefit.

**Expose iteration with range-over-func (Go 1.23), not a full slice or a `ForEach` callback.** Return `iter.Seq[V]` / `iter.Seq2[K,V]`; callers use ordinary `for range`. Beats returning `[]T` (allocates the whole collection even on early break) and ad-hoc callbacks (non-standard, not composable). The stdlib adopted it (`slices.All`/`Values`, `maps.Keys`/`Values`, composing with `slices.Collect`/`Sorted`). Contract: the iterator must stop and return as soon as `yield` returns `false`.

### Concurrency Gotchas

**Goroutine lifetimes must be deterministic; never silently start a background goroutine in a library.** Goroutines are not garbage collected — one blocked on a channel that never receives leaks for the life of the process. Code Review Comments: "make it clear when — or whether — goroutines exit." Treat lifetime as a contract: exit on `ctx.Done()` / a closed channel, or bound it to a function scope. The decision to start a background goroutine belongs to the application layer (main), not a library constructor.

```go
// BAD: a library constructor that silently leaks
func NewCache(size int) *Cache {
    c := &Cache{}
    go c.evictionLoop() // never stops; leaks when the cache is abandoned
    return c
}
```

**The memory model gives no happens-before on goroutine exit or unsynchronized flags.** Sequential consistency holds only for data-race-free programs. Starting a goroutine is synchronized before its first statement (writes before `go f()` are visible inside `f`), but goroutine *exit* carries no happens-before guarantee, and a flag read/written without a channel, mutex, or atomic has none. So `for !done {}` may loop forever (the value can sit in a register), and a write just before a goroutine exits is not guaranteed visible elsewhere. The `WaitGroup`/channel you observe completion through is the synchronizer — not termination itself. Use `sync.Once`, `sync/atomic`, a channel, or a mutex — and `go test -race` to catch violations.

**`WaitGroup.Add` must run before `go`.** Calling `wg.Add(1)` *inside* the goroutine races with `wg.Wait()` — `Wait` can return before the counter increments, stopping too early; the race detector does not reliably catch it. Increment before `go`. staticcheck flags this as SA2000. Go 1.25's `wg.Go(func(){...})` does Add(1) + launch + deferred Done atomically — prefer it for new code.

**Never copy a `sync` type after first use.** Every sync primitive (`Mutex`, `RWMutex`, `WaitGroup`, `Once`, `Cond`, `Map`, `Pool`) holds internal state a copy silently invalidates. Most common trigger: a **value receiver** on a method of a struct embedding one — each call locks a *copy*, so the lock protects nothing. Always use pointer receivers on such types. `go vet`'s copylock analyzer catches most cases (3-clause `for` coverage improved in Go 1.24).

```go
func (c Cache) Set(k string, v int) { // BAD: value receiver copies the mutex
    c.mu.Lock() // locks a copy; the real cache stays unprotected
    defer c.mu.Unlock()
    c.data[k] = v
}
```

### Context

**`context` keys must be an unexported package-local type, never a built-in.** A `string` or other built-in key lets any package using the same literal read or shadow your value (the `context` docs forbid built-in key types). An unexported named type makes the type system guarantee cross-package uniqueness — two packages each declaring `type ctxKey struct{}` produce distinct, non-equal key types. A zero-size `struct{}` key allocates nothing.

```go
type ctxKey struct{} // unexported; unique to this package
func WithUserID(ctx context.Context, id string) context.Context {
    return context.WithValue(ctx, ctxKey{}, id)
}
```

**Never store `context.Context` in a struct — pass it as the first argument.** A context encodes the lifetime and cancellation scope of one logical operation; storing it makes it ambiguous which operations it governs, denies per-call deadlines, and invites leaks (Go team's "Contexts and structs"). The one accepted exception is retrofitting an existing API (as `net/http.Request` did), and even then duplicate methods (`CallContext` vs `Call`) are preferred. Corollary: don't drop the request context and start a fresh `context.Background()` inside helpers — thread `ctx` through every boundary.

### Language Gotchas

**The typed-nil interface trap: return the `error` interface, not a concrete error pointer.** An interface value is `nil` only when both its type and value slots are unset. Assigning a typed nil pointer (`var p *MyError = nil`) to an `error` return makes the interface hold `(T=*MyError, V=nil)`, which is **non-nil** — every `if err != nil` at the call site then fires even on success. Declare the return type as the `error` interface and return a bare untyped `nil` on success; never the concrete pointer. Applies to any interface. staticcheck flags the always-non-nil comparison as SA4023.

```go
func returnsError() error {
    if bad() {
        return ErrBad // concrete value ONLY on the error path
    }
    return nil // bare untyped nil
}
```

**`defer` evaluates arguments immediately; a named return lets a deferred closure mutate the result.** `defer f(x)` evaluates `x` when the `defer` runs, not when `f` executes — so `defer fmt.Println(i)` captures `i`'s current value. A deferred *closure* with no arguments captures by reference and sees later mutations. Combined with a named return, this is the idiomatic way to augment an error after `return` runs:

```go
func doOp(id string) (err error) { // named return
    defer func() {
        if err != nil {
            err = fmt.Errorf("doOp %s: %w", id, err) // mutates result after return
        }
    }()
    return riskyOp(id)
}
```

**`defer` in a loop queues until the function returns — the failure is FD exhaustion, not just memory.** Every `defer` is queued until the enclosing *function* returns, so files opened in a loop stay open simultaneously and exhaust the OS file-descriptor limit long before the loop ends. Fix: extract a per-iteration function (preferred) or an inline IIFE so each `defer` runs per iteration.

```go
func processFile(path string) error {
    f, err := os.Open(path)
    if err != nil {
        return err
    }
    defer f.Close() // runs at end of THIS function, once per iteration
    return process(f)
}
for _, path := range files {
    if err := processFile(path); err != nil {
        return err
    }
}
```

**A subslice shares the parent's backing array and spare capacity — cap it or copy.** Reslicing never copies; a subslice inherits capacity extending into the parent's tail, so appending while capacity remains writes silently into the parent (no panic — just corruption). Two fixes: (1) the three-index full slice `s[low:high:max]` caps capacity to `max-low` so the first append beyond `high` reallocates — use it when returning a slice a caller will append to; (2) when keeping a small excerpt of a large buffer, `copy` into a fresh, exactly-sized slice so the large backing array can be GC'd.

```go
parent := []int{1, 2, 3, 4, 5}
child := parent[1:3]      // cap extends to end of parent
child = append(child, 99) // overwrites parent[3] silently -> [1 2 3 99 5]

func head(s []int) []int { return s[0:1:1] } // first append reallocates; can't reach parent
```

**Log errors with `slog.Any("err", err)`, not `slog.String("error", err.Error())`.** slog's built-in handlers special-case error-typed Attr values — `JSONHandler` calls `Error()`, `TextHandler` uses `fmt.Sprint` — so pre-stringifying is unnecessary and lossy: it discards the concrete type a custom handler or `LogValuer` could inspect. (`"err"` is convention, not a documented standard — pick a key and be consistent.)

**HTTP response bodies must be drained AND closed to reuse the connection.** Per `net/http`: "If the Body is not both read to EOF and closed, the ... RoundTripper may not be able to re-use a persistent TCP connection." `Close()` after a partial read does not return a keep-alive connection to the pool; with the default `DefaultMaxIdleConnsPerHost = 2`, leaking connections under load causes new dials and timeouts. Always `defer resp.Body.Close()` *and* consume the body — `io.ReadAll` when you need it, `io.Copy(io.Discard, resp.Body)` when you don't.

**Return errors, don't `panic`, for ordinary runtime failures.** `panic` is for programmer bugs (invariant violations), not for a missing file or bad input. Recover only at process/goroutine boundaries (e.g. HTTP recovery middleware). A library that panics on ordinary failures forces every caller to defer-recover.

### Version-Gated Behavior

Gated by the `go` directive in `go.mod`; older modules keep old behavior. The directive gates *language and runtime semantics*, not just the minimum toolchain — review release notes when bumping it.

**Go 1.22 scopes for-loop variables per iteration — delete `x := x`.** Before 1.22 a loop's variables were created once and mutated each iteration, so closures/goroutines/parallel subtests captured a shared variable; the fix was `tt := tt`. Go 1.22 creates fresh variables each iteration (all loop forms). In 1.22+ modules `tt := tt` is dead code `go fix ./...` removes. Hazard: bumping to 1.22 can make parallel subtests that passed only by reading the *last* iteration's value start failing — run `GOEXPERIMENT=loopvar go test ./...` first.

**Go 1.23 made timer/ticker channels unbuffered — the drain-before-Reset idiom can now deadlock.** Pre-1.23, timer channels had capacity 1, so a stale tick could buffer and the safe `Reset` idiom drained first. In 1.23 they are unbuffered and the runtime guarantees no stale value after `Stop`/`Reset`, so call `Reset` directly. An unconditional drain (`<-t.C` with nothing pending) now blocks forever; even `if !t.Stop() { <-t.C }` is no longer needed. Unstopped Timers/Tickers are now GC'd once unreferenced. (`GODEBUG=asynctimerchan=1` reverts.)

**Since Go 1.22 the global `math/rand` is ChaCha8Rand — but still use `crypto/rand` for secrets.** Go 1.20 auto-seeds from OS entropy; 1.22 backs it with ChaCha8Rand, so accidental `math/rand` use is "no longer a security catastrophe" — but it is *not* a substitute for `crypto/rand`. Use `crypto/rand` for any secret (`crypto/rand.Text()` in 1.24+, or read `rand.Reader`); use `math/rand/v2` (1.22+) for non-secret randomness. `math/rand.Seed` is deprecated and, if called, forces the weak Go 1 generator.

```go
token := rand.Text()             // crypto/rand, Go 1.24+: secret, base32, >=128 bits
idx := mathrand.IntN(len(items)) // math/rand/v2: non-secret
```

**Modernize as part of the upgrade workflow — run `go fix` after bumping the directive:**

```text
go fix -diff ./...   # preview
go fix ./...         # apply
```

It rewrites `interface{}`→`any`, `sort.Slice`→`slices.SortFunc`, atomic free-functions→typed atomics (`atomic.Int64`/`Bool`/`Pointer[T]`, which also fix 32-bit alignment footguns), the `x := x` removal, and `context.WithCancel` in tests→`t.Context()`. Also adopt: `testing.T.Context()`/`B.Loop()` (1.24), `runtime.AddCleanup` (1.24, preferred over `SetFinalizer`), and the `slices`/`maps`/`cmp` packages (1.21).

### Naming

**Package names: no stutter, no `util`/`common`/`helper` grab-bags.** A package name is always visible at the call site, so a symbol repeating it reads redundantly: `http.HTTPError`, `chubby.ChubbyFile`. Pick names that form a natural phrase — `http.Error`, `chubby.File`, `io.Reader`. Second smell: `util`/`common`/`helper`/`misc`/`base` say nothing about contents, force import aliases, and accumulate unrelated code — split by domain instead.

## Verification Checklists

### Before completing any Go change

- [ ] `gofmt`/`goimports` clean; `go vet ./...` passes (catches copylock, printf, nil-func)
- [ ] `go build ./...` and `go test ./...` green
- [ ] `go test -race ./...` on any code touching goroutines/shared state
- [ ] `golangci-lint`/`staticcheck` clean (SA2000 Add-before-go, SA4023 typed-nil) if configured
- [ ] Every returned error is `%w`-wrapped with context, or deliberately `%v`-flattened at a boundary
- [ ] No ignored errors (`_ =`) except deliberate, commented cases

### Concurrency review

- [ ] Every goroutine has a deterministic exit (`ctx.Done()`, closed channel, or bounded scope) — no library-level background goroutines
- [ ] `sync` types are never copied (pointer receivers; no value passing of structs embedding them)
- [ ] Shared state read/written only under a mutex/atomic/channel — no unsynchronized flags
- [ ] Channels closed by the sender exactly once; receivers never close
- [ ] `context.Context` is a first parameter, threaded through, never stored in a struct

### API/interface review

- [ ] Interfaces declared in the consumer; constructors return concrete types
- [ ] Functions are synchronous unless concurrency is the caller's explicit request
- [ ] Zero value usable, or a constructor exists because init is non-trivial
- [ ] `error` return type is the interface; success path returns bare `nil` (no typed-nil trap)
