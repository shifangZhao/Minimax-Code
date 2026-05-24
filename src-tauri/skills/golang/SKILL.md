---
name: golang-patterns
description: 惯用 Go 模式、最佳实践和约定，用于构建健壮、高效和可维护的 Go 应用程序。
origin: ECC
---

# Go 开发模式

用于构建健壮、高效和可维护应用程序的惯用 Go 模式和最佳实践。

## 激活时机

- 编写新的 Go 代码
- 审查 Go 代码
- 重构现有 Go 代码
- 设计 Go 包/模块

## 核心原则

### 1. 简单性和清晰性

Go 偏向简单性而非巧妙。代码应该明显且易读。

```go
// 好：清晰直接
func GetUser(id string) (*User, error) {
    user, err := db.FindUser(id)
    if err != nil {
        return nil, fmt.Errorf("get user %s: %w", id, err)
    }
    return user, nil
}

// 坏：过于巧妙
func GetUser(id string) (*User, error) {
    return func() (*User, error) {
        if u, e := db.FindUser(id); e == nil {
            return u, nil
        } else {
            return nil, e
        }
    }()
}
```

### 2. 让零值有用

设计类型使其零值无需初始化即可直接使用。

```go
// 好：零值有用
type Counter struct {
    mu    sync.Mutex
    count int // 零值是 0，可直接使用
}

func (c *Counter) Inc() {
    c.mu.Lock()
    c.count++
    c.mu.Unlock()
}

// 好：bytes.Buffer 零值可用
var buf bytes.Buffer
buf.WriteString("hello")

// 坏：需要初始化
type BadCounter struct {
    counts map[string]int // nil map 会 panic
}
```

### 3. 接受接口，返回结构体

函数应接受接口参数并返回具体类型。

```go
// 好：接受接口，返回具体类型
func ProcessData(r io.Reader) (*Result, error) {
    data, err := io.ReadAll(r)
    if err != nil {
        return nil, err
    }
    return &Result{Data: data}, nil
}

// 坏：返回接口（不必要地隐藏实现细节）
func ProcessData(r io.Reader) (io.Reader, error) {
    // ...
}
```

## 错误处理模式

### 带上下文的错误包装

```go
// 好：用上下文包装错误
func LoadConfig(path string) (*Config, error) {
    data, err := os.ReadFile(path)
    if err != nil {
        return nil, fmt.Errorf("load config %s: %w", path, err)
    }

    var cfg Config
    if err := json.Unmarshal(data, &cfg); err != nil {
        return nil, fmt.Errorf("parse config %s: %w", path, err)
    }

    return &cfg, nil
}
```

### 自定义错误类型

```go
// 定义领域特定错误
type ValidationError struct {
    Field   string
    Message string
}

func (e *ValidationError) Error() string {
    return fmt.Sprintf("validation failed on %s: %s", e.Field, e.Message)
}

// 哨兵错误用于常见情况
var (
    ErrNotFound     = errors.New("resource not found")
    ErrUnauthorized = errors.New("unauthorized")
    ErrInvalidInput = errors.New("invalid input")
)
```

### 使用 errors.Is 和 errors.As 进行错误检查

```go
func HandleError(err error) {
    // 检查特定错误
    if errors.Is(err, sql.ErrNoRows) {
        log.Println("No records found")
        return
    }

    // 检查错误类型
    var validationErr *ValidationError
    if errors.As(err, &validationErr) {
        log.Printf("Validation error on field %s: %s",
            validationErr.Field, validationErr.Message)
        return
    }

    // 未知错误
    log.Printf("Unexpected error: %v", err)
}
```

### 绝不忽略错误

```go
// 坏：用空白标识符忽略错误
result, _ := doSomething()

// 好：处理或明确说明为什么可以安全忽略
result, err := doSomething()
if err != nil {
    return err
}

// 可接受：当错误真的不重要时（很少）
_ = writer.Close() // 尽力清理，错误在其他地方记录
```

## 并发模式

### Worker Pool

```go
func WorkerPool(jobs <-chan Job, results chan<- Result, numWorkers int) {
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
```

### Context 用于取消和超时

```go
func FetchWithTimeout(ctx context.Context, url string) ([]byte, error) {
    ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
    defer cancel()

    req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
    if err != nil {
        return nil, fmt.Errorf("create request: %w", err)
    }

    resp, err := http.DefaultClient.Do(req)
    if err != nil {
        return nil, fmt.Errorf("fetch %s: %w", url, err)
    }
    defer resp.Body.Close()

    return io.ReadAll(resp.Body)
}
```

### 优雅关闭

```go
func GracefulShutdown(server *http.Server) {
    quit := make(chan os.Signal, 1)
    signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

    <-quit
    log.Println("Shutting down server...")

    ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
    defer cancel()

    if err := server.Shutdown(ctx); err != nil {
        log.Fatalf("Server forced to shutdown: %v", err)
    }

    log.Println("Server exited")
}
```

### errgroup 用于协调整 goroutine

```go
import "golang.org/x/sync/errgroup"

func FetchAll(ctx context.Context, urls []string) ([][]byte, error) {
    g, ctx := errgroup.WithContext(ctx)
    results := make([][]byte, len(urls))

    for i, url := range urls {
        i, url := i, url // 捕获循环变量
        g.Go(func() error {
            data, err := FetchWithTimeout(ctx, url)
            if err != nil {
                return err
            }
            results[i] = data
            return nil
        })
    }

    if err := g.Wait(); err != nil {
        return nil, err
    }
    return results, nil
}
```

### 避免 Goroutine 泄漏

```go
// 坏：如果 context 被取消则 goroutine 泄漏
func leakyFetch(ctx context.Context, url string) <-chan []byte {
    ch := make(chan []byte)
    go func() {
        data, _ := fetch(url)
        ch <- data // 如果没有接收者会永远阻塞
    }()
    return ch
}

// 好：正确处理取消
func safeFetch(ctx context.Context, url string) <-chan []byte {
    ch := make(chan []byte, 1) // 有缓冲通道
    go func() {
        data, err := fetch(url)
        if err != nil {
            return
        }
        select {
        case ch <- data:
        case <-ctx.Done():
        }
    }()
    return ch
}
```

## 接口设计

### 小而专注的接口

```go
// 好：单方法接口
type Reader interface {
    Read(p []byte) (n int, err error)
}

type Writer interface {
    Write(p []byte) (n int, err error)
}

type Closer interface {
    Close() error
}

// 根据需要组合接口
type ReadWriteCloser interface {
    Reader
    Writer
    Closer
}
```

### 在使用处定义接口

```go
// 在消费者包中，而非提供者
package service

// UserStore 定义此服务需要的内容
type UserStore interface {
    GetUser(id string) (*User, error)
    SaveUser(user *User) error
}

type Service struct {
    store UserStore
}

// 具体实现可以在另一个包中
// 它不需要知道这个接口
```

### 使用类型断言的可选行为

```go
type Flusher interface {
    Flush() error
}

func WriteAndFlush(w io.Writer, data []byte) error {
    if _, err := w.Write(data); err != nil {
        return err
    }

    // 如果支持则 Flush
    if f, ok := w.(Flusher); ok {
        return f.Flush()
    }
    return nil
}
```

## 包组织

### 标准项目布局

```text
myproject/
├── cmd/
│   └── myapp/
│       └── main.go           # 入口点
├── internal/
│   ├── handler/              # HTTP 处理器
│   ├── service/              # 业务逻辑
│   ├── repository/           # 数据访问
│   └── config/               # 配置
├── pkg/
│   └── client/               # 公共 API 客户端
├── api/
│   └── v1/                   # API 定义（proto、OpenAPI）
├── testdata/                 # 测试fixtures
├── go.mod
├── go.sum
└── Makefile
```

### 包命名

```go
// 好：短、小写、无下划线
package http
package json
package user

// 坏：冗长、混合大小写或多余
package httpHandler
package json_parser
package userService // 多余的 'Service' 后缀
```

### 避免包级状态

```go
// 坏：全局可变状态
var db *sql.DB

func init() {
    db, _ = sql.Open("postgres", os.Getenv("DATABASE_URL"))
}

// 好：依赖注入
type Server struct {
    db *sql.DB
}

func NewServer(db *sql.DB) *Server {
    return &Server{db: db}
}
```

## 结构体设计

### 函数式选项模式

```go
type Server struct {
    addr    string
    timeout time.Duration
    logger  *log.Logger
}

type Option func(*Server)

func WithTimeout(d time.Duration) Option {
    return func(s *Server) {
        s.timeout = d
    }
}

func WithLogger(l *log.Logger) Option {
    return func(s *Server) {
        s.logger = l
    }
}

func NewServer(addr string, opts ...Option) *Server {
    s := &Server{
        addr:    addr,
        timeout: 30 * time.Second, // 默认
        logger:  log.Default(),    // 默认
    }
    for _, opt := range opts {
        opt(s)
    }
    return s
}

// 使用
server := NewServer(":8080",
    WithTimeout(60*time.Second),
    WithLogger(customLogger),
)
```

### 通过嵌入组合

```go
type Logger struct {
    prefix string
}

func (l *Logger) Log(msg string) {
    fmt.Printf("[%s] %s\n", l.prefix, msg)
}

type Server struct {
    *Logger // 嵌入 - Server 获得 Log 方法
    addr    string
}

func NewServer(addr string) *Server {
    return &Server{
        Logger: &Logger{prefix: "SERVER"},
        addr:   addr,
    }
}

// 使用
s := NewServer(":8080")
s.Log("Starting...") // 调用嵌入的 Logger.Log
```

## 内存和性能

### 已知大小时预分配切片

```go
// 坏：多次增长切片
func processItems(items []Item) []Result {
    var results []Result
    for _, item := range items {
        results = append(results, transform(item))
    }
    return results
}

// 好：预先分配
func processItems(items []Item) []Result {
    results := make([]Result, len(items))
    for i, item := range items {
        results[i] = transform(item)
    }
    return results
}
```

### 字符串连接

```go
// 坏：循环中多次分配
var s string
for _, item := range items {
    s += item.Name + ","
}

// 好：使用 strings.Builder
var b strings.Builder
for _, item := range items {
    b.WriteString(item.Name)
    b.WriteString(",")
}
result := b.String()
```

## 测试模式

### 表驱动测试

```go
func TestFactorial(t *testing.T) {
    tests := []struct {
        name  string
        input int
        want  int
    }{
        {"zero", 0, 1},
        {"one", 1, 1},
        {"five", 5, 120},
        {"negative", -1, 0},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            got := Factorial(tt.input)
            if got != tt.want {
                t.Errorf("Factorial(%d) = %d; want %d", tt.input, got, tt.want)
            }
        })
    }
}
```

### 使用 httptest 测试 HTTP 处理程序

```go
func TestHealthEndpoint(t *testing.T) {
    handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        w.WriteHeader(http.StatusOK)
        json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
    })

    req := httptest.NewRequest("GET", "/health", nil)
    rr := httptest.NewRecorder()
    handler.ServeHTTP(rr, req)

    if rr.Code != http.StatusOK {
        t.Errorf("expected status 200, got %d", rr.Code)
    }
}
```

### 模拟接口

```go
type MockUserStore struct {
    users map[string]*User
}

func (m *MockUserStore) GetUser(id string) (*User, error) {
    if user, ok := m.users[id]; ok {
        return user, nil
    }
    return nil, ErrNotFound
}

func (m *MockUserStore) SaveUser(user *User) error {
    m.users[user.ID] = user
    return nil
}

// 使用
store := &MockUserStore{users: make(map[string]*User)}
service := NewUserService(store)
```

## 最佳实践总结

1. 简单胜于巧妙
2. 错误处理始终要显式
3. 优先组合而非继承
4. 保持接口小
5. 依赖注入而非全局状态
6. 文档化公共 API
7. 写表驱动测试
8. 避免goroutine泄漏
9. 使用 context 进行取消
10. 优先使用标准库