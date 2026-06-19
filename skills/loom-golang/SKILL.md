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
---

# Go Language Expertise

## Overview

This skill provides guidance for writing idiomatic, efficient, and production-quality Go code. It covers Go's concurrency model, error handling patterns, testing practices, and module system following Effective Go principles.

## Key Concepts

### Error Handling

```go
import (
    "errors"
    "fmt"
)

// Define sentinel errors
var (
    ErrNotFound     = errors.New("resource not found")
    ErrUnauthorized = errors.New("unauthorized access")
)

// Custom error types with context
type ValidationError struct {
    Field   string
    Message string
}

func (e *ValidationError) Error() string {
    return fmt.Sprintf("validation error on %s: %s", e.Field, e.Message)
}

// Error wrapping for context
func fetchUser(id string) (*User, error) {
    user, err := db.GetUser(id)
    if err != nil {
        if errors.Is(err, sql.ErrNoRows) {
            return nil, fmt.Errorf("user %s: %w", id, ErrNotFound)
        }
        return nil, fmt.Errorf("fetching user %s: %w", id, err)
    }
    return user, nil
}

// Error checking with Is and As
func handleError(err error) {
    if errors.Is(err, ErrNotFound) {
        // Handle not found
    }

    var validErr *ValidationError
    if errors.As(err, &validErr) {
        // Handle validation error with access to Field and Message
    }
}
```

### Concurrency Patterns

```go
// Worker pool pattern
func workerPool(jobs <-chan Job, results chan<- Result, numWorkers int) {
    var wg sync.WaitGroup
    for i := 0; i < numWorkers; i++ {
        wg.Add(1)
        go func() {
            defer wg.Done()
            for job := range jobs {
                results <- process(job)
            }
        }()
    }
    wg.Wait()
    close(results)
}

// Go 1.25+: sync.WaitGroup.Go combines Add(1) + launch + Done, making the
// "Add inside the goroutine" race (staticcheck SA2000) structurally impossible.
func workerPoolGo125(jobs <-chan Job, results chan<- Result, numWorkers int) {
    var wg sync.WaitGroup
    for i := 0; i < numWorkers; i++ {
        wg.Go(func() {
            for job := range jobs {
                results <- process(job)
            }
        })
    }
    wg.Wait()
    close(results)
}

// Context for cancellation and timeouts
func fetchWithTimeout(ctx context.Context, url string) ([]byte, error) {
    ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
    defer cancel()

    req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
    if err != nil {
        return nil, err
    }

    resp, err := http.DefaultClient.Do(req)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()

    return io.ReadAll(resp.Body)
}

// Select for multiple channels
func multiplex(ctx context.Context, ch1, ch2 <-chan int) <-chan int {
    out := make(chan int)
    go func() {
        defer close(out)
        for {
            select {
            case v, ok := <-ch1:
                if !ok {
                    ch1 = nil
                    continue
                }
                out <- v
            case v, ok := <-ch2:
                if !ok {
                    ch2 = nil
                    continue
                }
                out <- v
            case <-ctx.Done():
                return
            }
            if ch1 == nil && ch2 == nil {
                return
            }
        }
    }()
    return out
}

// Mutex for shared state
type SafeCounter struct {
    mu    sync.RWMutex
    count map[string]int
}

func (c *SafeCounter) Inc(key string) {
    c.mu.Lock()
    defer c.mu.Unlock()
    c.count[key]++
}

func (c *SafeCounter) Get(key string) int {
    c.mu.RLock()
    defer c.mu.RUnlock()
    return c.count[key]
}
```

### Interfaces and Embedding

```go
// Small, focused interfaces
type Reader interface {
    Read(p []byte) (n int, err error)
}

type Writer interface {
    Write(p []byte) (n int, err error)
}

type ReadWriter interface {
    Reader
    Writer
}

// Accept interfaces, return structs
type UserRepository interface {
    GetByID(ctx context.Context, id string) (*User, error)
    Create(ctx context.Context, user *User) error
}

type userService struct {
    repo   UserRepository
    cache  Cache
    logger *slog.Logger
}

func NewUserService(repo UserRepository, cache Cache, logger *slog.Logger) *userService {
    return &userService{
        repo:   repo,
        cache:  cache,
        logger: logger,
    }
}

// Embedding for composition
type Base struct {
    ID        string
    CreatedAt time.Time
    UpdatedAt time.Time
}

type User struct {
    Base
    Email string
    Name  string
}
```

### Functional Options Pattern

```go
type Server struct {
    host    string
    port    int
    timeout time.Duration
    logger  *slog.Logger
}

type Option func(*Server)

func WithHost(host string) Option {
    return func(s *Server) {
        s.host = host
    }
}

func WithPort(port int) Option {
    return func(s *Server) {
        s.port = port
    }
}

func WithTimeout(d time.Duration) Option {
    return func(s *Server) {
        s.timeout = d
    }
}

func WithLogger(logger *slog.Logger) Option {
    return func(s *Server) {
        s.logger = logger
    }
}

func NewServer(opts ...Option) *Server {
    s := &Server{
        host:    "localhost",
        port:    8080,
        timeout: 30 * time.Second,
        logger:  slog.Default(),
    }
    for _, opt := range opts {
        opt(s)
    }
    return s
}

// Usage
server := NewServer(
    WithHost("0.0.0.0"),
    WithPort(9000),
    WithTimeout(60*time.Second),
)
```

### CLI Applications with Cobra

```go
// cmd/root.go
package cmd

import (
    "fmt"
    "os"

    "github.com/spf13/cobra"
    "github.com/spf13/viper"
)

var (
    cfgFile string
    verbose bool
)

var rootCmd = &cobra.Command{
    Use:   "myapp",
    Short: "A brief description of your application",
    Long: `A longer description that spans multiple lines and likely contains
examples and usage of using your application.`,
}

func Execute() {
    if err := rootCmd.Execute(); err != nil {
        fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}

func init() {
    cobra.OnInitialize(initConfig)

    rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.myapp.yaml)")
    rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "verbose output")

    viper.BindPFlag("verbose", rootCmd.PersistentFlags().Lookup("verbose"))
}

func initConfig() {
    if cfgFile != "" {
        viper.SetConfigFile(cfgFile)
    } else {
        home, err := os.UserHomeDir()
        cobra.CheckErr(err)
        viper.AddConfigPath(home)
        viper.SetConfigType("yaml")
        viper.SetConfigName(".myapp")
    }

    viper.AutomaticEnv()

    if err := viper.ReadInConfig(); err == nil {
        fmt.Fprintln(os.Stderr, "Using config file:", viper.ConfigFileUsed())
    }
}

// cmd/serve.go
var serveCmd = &cobra.Command{
    Use:   "serve",
    Short: "Start the HTTP server",
    RunE: func(cmd *cobra.Command, args []string) error {
        port := viper.GetInt("port")
        return startServer(cmd.Context(), port)
    },
}

func init() {
    rootCmd.AddCommand(serveCmd)
    serveCmd.Flags().IntP("port", "p", 8080, "Port to listen on")
    viper.BindPFlag("port", serveCmd.Flags().Lookup("port"))
}
```

### HTTP Servers

```go
// Standard library HTTP server
package server

import (
    "context"
    "errors"
    "log/slog"
    "net/http"
    "time"
)

type Server struct {
    httpServer *http.Server
    logger     *slog.Logger
}

func New(addr string, handler http.Handler, logger *slog.Logger) *Server {
    return &Server{
        httpServer: &http.Server{
            Addr:         addr,
            Handler:      handler,
            ReadTimeout:  15 * time.Second,
            WriteTimeout: 15 * time.Second,
            IdleTimeout:  60 * time.Second,
        },
        logger: logger,
    }
}

func (s *Server) Start() error {
    s.logger.Info("starting server", slog.String("addr", s.httpServer.Addr))
    if err := s.httpServer.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
        return err
    }
    return nil
}

func (s *Server) Shutdown(ctx context.Context) error {
    s.logger.Info("shutting down server")
    return s.httpServer.Shutdown(ctx)
}

// Middleware patterns
func loggingMiddleware(logger *slog.Logger) func(http.Handler) http.Handler {
    return func(next http.Handler) http.Handler {
        return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
            start := time.Now()
            next.ServeHTTP(w, r)
            logger.Info("request",
                slog.String("method", r.Method),
                slog.String("path", r.URL.Path),
                slog.Duration("duration", time.Since(start)),
            )
        })
    }
}

func recoveryMiddleware(logger *slog.Logger) func(http.Handler) http.Handler {
    return func(next http.Handler) http.Handler {
        return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
            defer func() {
                if err := recover(); err != nil {
                    logger.Error("panic recovered",
                        slog.Any("error", err),
                        slog.String("path", r.URL.Path),
                    )
                    http.Error(w, "internal server error", http.StatusInternalServerError)
                }
            }()
            next.ServeHTTP(w, r)
        })
    }
}

// Router setup with Go 1.22+ patterns
func setupRoutes(mux *http.ServeMux, handler *Handler) {
    mux.HandleFunc("GET /api/users/{id}", handler.GetUser)
    mux.HandleFunc("POST /api/users", handler.CreateUser)
    mux.HandleFunc("PUT /api/users/{id}", handler.UpdateUser)
    mux.HandleFunc("DELETE /api/users/{id}", handler.DeleteUser)
    mux.HandleFunc("GET /health", handler.Health)
}

// Gin framework example
import "github.com/gin-gonic/gin"

func setupGinRouter(handler *Handler) *gin.Engine {
    r := gin.New()
    r.Use(gin.Recovery())
    r.Use(gin.Logger())

    api := r.Group("/api")
    {
        users := api.Group("/users")
        {
            users.GET("/:id", handler.GetUser)
            users.POST("/", handler.CreateUser)
            users.PUT("/:id", handler.UpdateUser)
            users.DELETE("/:id", handler.DeleteUser)
        }
    }

    r.GET("/health", handler.Health)
    return r
}
```

### Database Access Patterns

```go
// Using sqlx for cleaner database operations
import (
    "context"
    "database/sql"
    "github.com/jmoiron/sqlx"
    _ "github.com/lib/pq"
)

type User struct {
    ID        string    `db:"id"`
    Email     string    `db:"email"`
    Name      string    `db:"name"`
    CreatedAt time.Time `db:"created_at"`
}

type UserRepository struct {
    db *sqlx.DB
}

func NewUserRepository(db *sqlx.DB) *UserRepository {
    return &UserRepository{db: db}
}

func (r *UserRepository) GetByID(ctx context.Context, id string) (*User, error) {
    var user User
    query := `SELECT id, email, name, created_at FROM users WHERE id = $1`

    if err := r.db.GetContext(ctx, &user, query, id); err != nil {
        if errors.Is(err, sql.ErrNoRows) {
            return nil, ErrNotFound
        }
        return nil, fmt.Errorf("getting user: %w", err)
    }
    return &user, nil
}

func (r *UserRepository) Create(ctx context.Context, user *User) error {
    query := `
        INSERT INTO users (id, email, name, created_at)
        VALUES (:id, :email, :name, :created_at)
    `

    _, err := r.db.NamedExecContext(ctx, query, user)
    if err != nil {
        return fmt.Errorf("creating user: %w", err)
    }
    return nil
}

func (r *UserRepository) List(ctx context.Context, limit, offset int) ([]*User, error) {
    var users []*User
    query := `
        SELECT id, email, name, created_at
        FROM users
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
    `

    if err := r.db.SelectContext(ctx, &users, query, limit, offset); err != nil {
        return nil, fmt.Errorf("listing users: %w", err)
    }
    return users, nil
}

// Transaction helper
func (r *UserRepository) WithTx(ctx context.Context, fn func(*sqlx.Tx) error) error {
    tx, err := r.db.BeginTxx(ctx, nil)
    if err != nil {
        return fmt.Errorf("beginning transaction: %w", err)
    }

    if err := fn(tx); err != nil {
        if rbErr := tx.Rollback(); rbErr != nil {
            return fmt.Errorf("rolling back transaction: %v (original error: %w)", rbErr, err)
        }
        return err
    }

    if err := tx.Commit(); err != nil {
        return fmt.Errorf("committing transaction: %w", err)
    }
    return nil
}
```

## Best Practices

### Code Organization

```text
myproject/
├── cmd/
│   └── myapp/
│       └── main.go
├── internal/
│   ├── service/
│   ├── repository/
│   └── handler/
├── pkg/
│   └── utils/
├── go.mod
└── go.sum
```

### Module Management

```go
// go.mod
module github.com/user/myproject

go 1.25

require (
    github.com/lib/pq v1.10.9
    golang.org/x/sync v0.5.0
)
```

The `go` directive gates language and runtime semantics, not just the minimum toolchain — bumping it activates behavior changes (e.g. per-iteration loop variables at 1.22, unbuffered timer channels at 1.23), so review the release notes when raising it.

### Structured Logging with slog

```go
import "log/slog"

logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{
    Level: slog.LevelInfo,
}))

logger.Info("request processed",
    slog.String("method", r.Method),
    slog.String("path", r.URL.Path),
    slog.Duration("latency", time.Since(start)),
)

// Add context to logger
logger = logger.With(slog.String("request_id", requestID))
```

## Common Patterns

### Table-Driven Tests

```go
func TestAdd(t *testing.T) {
    tests := []struct {
        name     string
        a, b     int
        expected int
    }{
        {"positive numbers", 2, 3, 5},
        {"negative numbers", -1, -2, -3},
        {"zero", 0, 0, 0},
        {"mixed", -1, 5, 4},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            result := Add(tt.a, tt.b)
            if result != tt.expected {
                t.Errorf("Add(%d, %d) = %d; want %d", tt.a, tt.b, result, tt.expected)
            }
        })
    }
}

// Parallel subtests
func TestFetch(t *testing.T) {
    tests := []struct {
        name string
        url  string
    }{
        {"google", "https://google.com"},
        {"github", "https://github.com"},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            t.Parallel()
            // test implementation
        })
    }
}
```

In modules declaring `go 1.22` or later, range-loop variables are per-iteration, so the old `tt := tt` workaround is unneeded and is removed by `go fix ./...`; it is required only for modules declaring `go < 1.22`. Bumping an existing module to 1.22 can surface latent test bugs (parallel subtests that passed only by accidentally reading the last iteration's value) — run `GOEXPERIMENT=loopvar go test ./...` before the bump to find them.

### Benchmarks

```go
// Go 1.24+: prefer for b.Loop() over the b.N loop. Setup before the loop runs
// exactly once per -count (not b.N times), and the runtime keeps params/results
// alive so the compiler cannot elide the body — so a separate b.ResetTimer for
// that purpose is no longer needed.
func BenchmarkFibonacci(b *testing.B) {
    for b.Loop() {
        Fibonacci(20)
    }
}

func BenchmarkFibonacciParallel(b *testing.B) {
    b.RunParallel(func(pb *testing.PB) {
        for pb.Next() {
            Fibonacci(20)
        }
    })
}

// With sub-benchmarks
func BenchmarkSort(b *testing.B) {
    sizes := []int{100, 1000, 10000}
    for _, size := range sizes {
        b.Run(fmt.Sprintf("size-%d", size), func(b *testing.B) {
            data := generateData(size) // runs once per -count with b.Loop()
            for b.Loop() {
                Sort(data)
            }
        })
    }
}
```

### HTTP Handlers

```go
func (h *Handler) GetUser(w http.ResponseWriter, r *http.Request) {
    ctx := r.Context()
    id := r.PathValue("id") // Go 1.22+

    user, err := h.service.GetUser(ctx, id)
    if err != nil {
        if errors.Is(err, ErrNotFound) {
            http.Error(w, "user not found", http.StatusNotFound)
            return
        }
        h.logger.Error("failed to get user", slog.Any("err", err))
        http.Error(w, "internal error", http.StatusInternalServerError)
        return
    }

    w.Header().Set("Content-Type", "application/json")
    if err := json.NewEncoder(w).Encode(user); err != nil {
        h.logger.Error("failed to encode response", slog.Any("err", err))
    }
}
```

## Anti-Patterns

### Avoid These Practices

```go
// BAD: Ignoring errors
result, _ := doSomething()

// GOOD: Always handle errors
result, err := doSomething()
if err != nil {
    return fmt.Errorf("doing something: %w", err)
}

// BAD: Goroutine leaks
func fetch(urls []string) []Result {
    results := make(chan Result)
    for _, url := range urls {
        go func(u string) {
            results <- fetchURL(u) // Blocks forever if nobody reads
        }(url)
    }
    return collectResults(results)
}

// GOOD: Use context and proper cleanup
func fetch(ctx context.Context, urls []string) ([]Result, error) {
    g, ctx := errgroup.WithContext(ctx)
    results := make([]Result, len(urls))

    for i, url := range urls {
        i, url := i, url
        g.Go(func() error {
            r, err := fetchURL(ctx, url)
            if err != nil {
                return err
            }
            results[i] = r
            return nil
        })
    }

    if err := g.Wait(); err != nil {
        return nil, err
    }
    return results, nil
}

// BAD: Returning interfaces
func NewService() ServiceInterface {
    return &service{}
}

// GOOD: Return concrete types
func NewService() *Service {
    return &Service{}
}
// Why it's a contract, not style: returning an interface forces the producer's
// method set onto every consumer (pre-empting the minimal interface each one
// would otherwise declare) and blocks additive change — a new method on the
// concrete type can't be used without widening the interface, a breaking change.
// Return a concrete type and let each consumer declare the small interface it
// needs. An exported constructor returning an UNEXPORTED concrete type
// (func New() *myType) is idiomatic.

// BAD: Large interfaces
type Repository interface {
    GetUser(id string) (*User, error)
    CreateUser(user *User) error
    UpdateUser(user *User) error
    DeleteUser(id string) error
    ListUsers() ([]*User, error)
    GetOrder(id string) (*Order, error)
    // ... 20 more methods
}

// GOOD: Small, focused interfaces
type UserGetter interface {
    GetUser(ctx context.Context, id string) (*User, error)
}

// BAD: Naked returns in long functions
func process(data []byte) (result string, err error) {
    // 50 lines of code
    result = string(data)
    return // What's being returned?
}

// GOOD: Explicit returns
func process(data []byte) (string, error) {
    // processing logic
    return string(data), nil
}

// BAD: init() for complex initialization
func init() {
    db = connectToDatabase()
    cache = initCache()
}

// GOOD: Explicit initialization in main
func main() {
    db, err := connectToDatabase()
    if err != nil {
        log.Fatal(err)
    }
    defer db.Close()

    cache := initCache()
    // ...
}
```

### Quick Pattern Swaps

```go
// BAD: Dropping request context inside helpers
func loadUser(id string) (*User, error) {
    ctx := context.Background()
    return repo.GetUser(ctx, id)
}

// GOOD: Thread context through every boundary
func loadUser(ctx context.Context, id string) (*User, error) {
    return repo.GetUser(ctx, id)
}

// BAD: Panic for ordinary runtime failures
func parseConfig(path string) Config {
    b, _ := os.ReadFile(path)
    return mustParseConfig(b)
}

// GOOD: Return errors and let the caller decide
func parseConfig(path string) (Config, error) {
    b, err := os.ReadFile(path)
    if err != nil {
        return Config{}, err
    }
    return parseConfigBytes(b)
}

// BAD: Deferring cleanup inside a long loop. Every defer is queued until the
// FUNCTION returns, so all the files stay open SIMULTANEOUSLY — the dominant
// failure is exhausting the OS file-descriptor limit long before the loop ends,
// not just memory from queued defers.
for _, path := range files {
    f, _ := os.Open(path)
    defer f.Close()
    process(f)
}

// GOOD (preferred): extract a named function so each defer runs per iteration
func processFile(path string) error {
    f, err := os.Open(path)
    if err != nil {
        return err
    }
    defer f.Close()
    return process(f)
}
for _, path := range files {
    if err := processFile(path); err != nil {
        return err
    }
}

// Also fine: an inline IIFE that scopes the defer to one iteration
for _, path := range files {
    if err := func() error {
        f, err := os.Open(path)
        if err != nil {
            return err
        }
        defer f.Close()
        return process(f)
    }(); err != nil {
        return err
    }
}
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

The patterns above get code working; the ones below are what separates idiomatic, production-grade Go from code that compiles. Each includes the mechanism — the *why* — because that is what lets you apply the rule to cases not shown here.

### Design Patterns

**Define interfaces in the consumer; return concrete types from the producer.** Go interfaces are satisfied implicitly, so a producer never needs to declare the interface its types satisfy. Per Go Code Review Comments: "Go interfaces generally belong in the package that uses values of the interface type, not the package that implements those values," and "do not define interfaces before they are used." A broad producer-side interface (created for mocking) couples every consumer to that shape instead of the minimal method set it actually uses. Idiomatic: the producer returns a concrete type (often `*T`, frequently an unexported struct behind an exported `func New() *myType`) so new methods can be added without breaking callers; each consumer declares the small interface it needs.

```go
// consumer/service.go — declares exactly what it uses
type UserGetter interface {
    GetUser(ctx context.Context, id string) (*User, error)
}
func NewService(g UserGetter) *Service { /* ... */ }

// producer/repo.go — returns a concrete type, idiomatic even when unexported
func NewUserRepository(db *sqlx.DB) *userRepository { /* ... */ }
```

**Prefer synchronous functions; let callers add concurrency.** Code Review Comments: "Prefer synchronous functions over asynchronous ones." Hiding goroutine creation forces concurrency on every caller — they lose control over lifetime, can't add timeouts, can't call synchronously, and the function is harder to test. The asymmetry is the point: making a sync function async is one line at the call site (`go func(){ errs <- Process(ctx, item) }()`), but un-async-ing an async API requires rewriting every caller (sometimes impossible).

**Design types so the zero value is usable.** Every Go variable is zero-initialized; expert APIs exploit this so a type needs no constructor and resists init-order bugs. The stdlib does this pervasively — `sync.Mutex` (zero = unlocked), `bytes.Buffer`, `sync.WaitGroup` all work at zero value with no `Init()`. Corollaries: name boolean fields so the zero value (`false`) is the safe default (prefer `disabled` over `enabled`); add a constructor only when initialization is genuinely non-trivial, and have it return a concrete type. Anti-patterns: requiring an `Init()` call before use, or panicking on a zero value.

**`%w` makes the wrapped error part of your public API; use `%v` at boundaries.** `fmt.Errorf("...: %w", err)` lets callers `errors.Is`/`errors.As` the wrapped value, making it part of your package's contract. Wrap an internal sentinel like `sql.ErrNoRows` with `%w` and callers can now depend on it — swapping your DB driver becomes a breaking change. This is an API decision, not a verbosity one. Use `%w` within an application or when the wrapped sentinel is genuinely meant to be inspectable; at package/system boundaries (RPC, storage, external services) use `%v` to flatten to a string, or convert to your own exported sentinel first. Caveat: `%v` destroys chain identity — a later `errors.Is` against the original sentinel returns `false` with no compile error.

```go
var ErrNotFound = errors.New("not found")

func GetUser(id string) (*User, error) {
    u, err := db.Get(id)
    if errors.Is(err, sql.ErrNoRows) {
        return nil, fmt.Errorf("user %s: %w", id, ErrNotFound) // YOUR sentinel, not sql.ErrNoRows
    }
    return u, err
}
```

**Use generics for type-identical code; use an interface when you only call methods.** Reach for type parameters when you'd otherwise write the same code differing only by concrete type — general-purpose containers and functions over slices/maps/channels of any element type. But (Go team's "When To Use Generics"): "if all you need to do with a value is call a method on it, use an interface type, not a type parameter." `func ReadSome[T io.Reader](r T)` is strictly worse than `func ReadSome(r io.Reader)` — same speed, harder to read, no benefit. Reserve generics for when the type itself is the data being stored or operated on element-wise.

**Expose iteration with range-over-func (Go 1.23), not a full slice or a `ForEach` callback.** A type exposes iteration by returning `iter.Seq[V]` / `iter.Seq2[K,V]`; callers use ordinary `for range`. This beats returning `[]T` (allocates the whole collection even if the caller breaks early) and ad-hoc `ForEach(func(v) bool)` callbacks (non-standard, not composable). The stdlib adopted it (`slices.All`/`Values`, `maps.Keys`/`Values`, composing with `slices.Collect`/`Sorted`). Contract: the iterator must stop producing and return as soon as `yield` returns `false`.

### Concurrency Gotchas

**Goroutine lifetimes must be deterministic; never silently start a background goroutine in a library.** Goroutines are not garbage collected — one blocked on a channel that never receives leaks for the life of the process. Code Review Comments: "make it clear when — or whether — goroutines exit; if that isn't feasible, document when and why." Treat lifetime as a contract: exit on `ctx.Done()` / a closed channel, or bound it to a function scope. The decision to start a background goroutine belongs to the application layer (main), not a library constructor.

```go
// BAD: a library constructor that silently leaks
func NewCache(size int) *Cache {
    c := &Cache{}
    go c.evictionLoop() // never stops; leaks when the cache is abandoned
    return c
}
```

**The memory model gives no happens-before on goroutine exit or unsynchronized flags.** Sequential consistency holds only for data-race-free programs. Starting a goroutine is synchronized before its first statement (writes before `go f()` are visible inside `f`), but goroutine *exit* carries no happens-before guarantee, and a flag read/written without a channel, mutex, or atomic has none. So `for !done {}` may loop forever (the value can be cached in a register), and a write made just before a goroutine exits is not guaranteed visible elsewhere. The `WaitGroup`/channel you observe completion through is the synchronizer — not goroutine termination. Use `sync.Once`, `sync/atomic`, a channel, or a mutex.

**`WaitGroup.Add` must run before `go`.** Calling `wg.Add(1)` *inside* the goroutine races with `wg.Wait()` — `Wait` can return before the counter is incremented, stopping too early; the race detector does not reliably catch it. Increment in the launching goroutine, before `go`. staticcheck flags this as SA2000. Go 1.25's `wg.Go(func(){...})` does Add(1) + launch + deferred Done atomically, making the race structurally impossible — prefer it for new code.

**Never copy a `sync` type after first use.** Every sync primitive (`Mutex`, `RWMutex`, `WaitGroup`, `Once`, `Cond`, `Map`, `Pool`) holds internal state that a copy silently invalidates. The most common trigger is a **value receiver** on a method of a struct embedding one — each call locks a *copy*, so the lock protects nothing. Always use pointer receivers on such types. `go vet`'s copylock analyzer catches most cases (3-clause `for` loop coverage improved in Go 1.24).

```go
func (c Cache) Set(k string, v int) { // BAD: value receiver copies the mutex
    c.mu.Lock()  // locks a copy; the real cache stays unprotected
    defer c.mu.Unlock()
    c.data[k] = v
}
```

### Context

**`context` keys must be an unexported package-local type, never a built-in.** A `string` or other built-in key lets any package using the same literal read or shadow your value (the `context` docs explicitly forbid built-in key types). Define an unexported named type so the type system guarantees cross-package uniqueness — two packages each declaring `type ctxKey struct{}` produce distinct, non-equal key types. A zero-size `struct{}` key allocates nothing (one key per package); use an unexported int/iota type for several.

```go
type ctxKey struct{} // unexported; unique to this package

func WithUserID(ctx context.Context, id string) context.Context {
    return context.WithValue(ctx, ctxKey{}, id)
}
```

**Never store `context.Context` in a struct — pass it as the first argument.** A context encodes the lifetime and cancellation scope of one logical operation; a stored context makes it ambiguous which operations it governs, denies callers per-call deadlines, and invites leaks in servers (Go team's "Contexts and structs"). Pass `ctx context.Context` as the first parameter of each call. The one accepted exception is retrofitting an existing API for compatibility (as `net/http.Request` did), and even then duplicate methods (`CallContext` vs `Call`) are preferred over struct storage.

### Language Gotchas

**The typed-nil interface trap: return the `error` interface, not a concrete error pointer.** An interface value is `nil` only when both its type slot and value slot are unset. Assigning a typed nil pointer (`var p *MyError = nil`) to an `error` return makes the interface hold `(T=*MyError, V=nil)`, which is **non-nil** — every `if err != nil` at the call site then fires even on success. Declare the function's return type as the `error` interface and return a bare untyped `nil` on the success path; never return the concrete pointer type. Applies to any interface, not just `error`. staticcheck flags the always-non-nil comparison as SA4023.

```go
func returnsError() error {
    if bad() {
        return ErrBad // concrete value ONLY on the error path
    }
    return nil // bare untyped nil
}
```

**`defer` evaluates arguments immediately; a named return lets a deferred closure mutate the result.** `defer f(x)` evaluates `x` when the `defer` statement runs, not when `f` executes — so `defer fmt.Println(i)` captures `i`'s current value. A deferred *closure* with no arguments, by contrast, captures by reference and sees later mutations. Combined with a named return value, this is the idiomatic way to augment an error with context after `return` has run:

```go
func doOp(id string) (err error) { // named return
    defer func() {
        if err != nil {
            err = fmt.Errorf("doOp %s: %w", id, err) // mutates the result after return
        }
    }()
    return riskyOp(id)
}
```

**A subslice shares the parent's backing array and spare capacity — cap it or copy.** Reslicing never copies; a subslice inherits capacity extending into the parent's tail, so appending to it while capacity remains writes silently into the parent (no panic — just corruption). Two fixes: (1) the three-index full slice expression `s[low:high:max]` caps capacity to `max-low` so the first append beyond `high` reallocates — use it when returning a slice a caller will append to; (2) when you keep a small excerpt of a large buffer, `copy` into a fresh, exactly-sized slice so the large backing array can be GC'd.

```go
parent := []int{1, 2, 3, 4, 5}
child := parent[1:3]      // cap extends to end of parent
child = append(child, 99) // overwrites parent[3] silently -> [1 2 3 99 5]

func head(s []int) []int { return s[0:1:1] } // first append reallocates, can't reach parent
```

**Log errors with `slog.Any("err", err)`, not `slog.String("error", err.Error())`.** slog's built-in handlers special-case error-typed Attr values — `JSONHandler` calls `Error()`, `TextHandler` uses `fmt.Sprint` — so pre-stringifying is unnecessary and lossy: it discards the concrete type, which a custom handler or `LogValuer` could otherwise inspect. Use `slog.Any`. On hot paths use `slog.LogAttrs` to avoid allocations. (`"err"` is a common convention, not a documented standard — pick a key and be consistent.)

**HTTP response bodies must be drained AND closed to reuse the connection.** Per `net/http`: "If the Body is not both read to EOF and closed, the Client's underlying RoundTripper ... may not be able to re-use a persistent TCP connection." `Close()` after a partial read does not return a keep-alive connection to the pool, and with the default `DefaultMaxIdleConnsPerHost = 2`, leaking connections under load causes new dials and timeouts. Always `defer resp.Body.Close()` *and* consume the body — `io.ReadAll` when you need it, `io.Copy(io.Discard, resp.Body)` when you don't.

### Currency: Version-Gated Behavior

These are gated by the `go` directive in `go.mod`; older modules keep the old behavior. Review release notes when bumping the directive.

**Go 1.22 scopes for-loop variables per iteration — delete `x := x`.** Before 1.22 a loop's variables were created once and mutated each iteration, so closures/goroutines/parallel subtests captured a shared variable; the standard fix was `tt := tt`. Go 1.22 creates fresh variables each iteration (all loop forms, including 3-clause `for`). In 1.22+ modules `tt := tt` is dead code that `go fix ./...` removes. Migration hazard: bumping to 1.22 can make parallel subtests that passed only by reading the last iteration's value start failing — run `GOEXPERIMENT=loopvar go test ./...` first.

**Go 1.23 made timer/ticker channels unbuffered — the drain-before-Reset idiom can now deadlock.** Pre-1.23, timer channels had capacity 1, so a stale tick could be buffered and the safe `Reset` idiom drained first. In 1.23 they are unbuffered and the runtime guarantees no stale value is sent or received after `Stop`/`Reset`, so call `Reset` directly. An unconditional drain (`<-t.C` with nothing pending) now blocks forever; even `if !t.Stop() { <-t.C }` is no longer needed. Also: unstopped Timers/Tickers are now GC'd once unreferenced. (`GODEBUG=asynctimerchan=1` reverts it.)

**Since Go 1.22 the global `math/rand` is ChaCha8Rand — but still use `crypto/rand` for secrets.** Pre-1.20 the global was a deterministic LFSR seeded from time (observing enough output predicted all future values). Go 1.20 auto-seeds from OS entropy; 1.22 backs it with ChaCha8Rand. Accidental `math/rand` use is now "no longer a security catastrophe" — but it is *not* a substitute for `crypto/rand`. Use `crypto/rand` for any secret (`crypto/rand.Text()` in 1.24+, or read `rand.Reader`); use `math/rand/v2` (1.22+) for non-secret randomness. `math/rand.Seed` is deprecated and, if called, forces the weak Go 1 generator.

```go
import "crypto/rand"
token := rand.Text() // Go 1.24+: secret, base32, >=128 bits

import mathrand "math/rand/v2"
idx := mathrand.IntN(len(items)) // non-secret
```

**Modernize as part of the upgrade workflow.** Each release supersedes older idioms, and the modernize analyzer applies many mechanically — run it *after* bumping the `go` directive, not as a one-off:

```text
go fix -diff ./...   # preview
go fix ./...         # apply
```

It rewrites `interface{}`→`any`, `sort.Slice`→`slices.SortFunc`, atomic free-functions→typed atomics (`atomic.Int64`/`Bool`/`Pointer[T]`, Go 1.19+, which also fix 32-bit alignment footguns), the `x := x` loop workaround removal, and `context.WithCancel` in tests→`t.Context()`. Other release idioms worth adopting: `testing.T.Context()` (1.24, auto-cancelled at test end), `testing.B.Loop()` (1.24, see Benchmarks above), `runtime.AddCleanup` (1.24 — docs say "new code should prefer AddCleanup over SetFinalizer"), and the `slices`/`maps`/`cmp` packages (1.21).

### Naming

**Package names: no stutter, no `util`/`common`/`helper` grab-bags.** A package name is always visible at the call site, so a symbol repeating it reads redundantly: `http.HTTPError`, `chubby.ChubbyFile`. Pick package + symbol names that form a natural phrase — `http.Error`, `chubby.File`, `io.Reader`. Second smell: packages named `util`/`common`/`helper`/`misc`/`base` say nothing about their contents, force import aliases, and accumulate unrelated code — split by domain instead.
