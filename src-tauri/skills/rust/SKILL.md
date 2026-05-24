---
name: rust-patterns
description: 惯用 Rust 模式、所有权、错误处理、trait、并发和用于构建安全、高性能应用程序的最佳实践。
origin: ECC
---

# Rust 开发模式

用于构建安全、高性能和可维护应用程序的惯用 Rust 模式和最佳实践。

## 激活时机

- 编写新的 Rust 代码
- 审查 Rust 代码
- 重构现有 Rust 代码
- 设计 crate 结构和模块布局

## 工作原理

此技能在六个关键领域强制执行惯用 Rust 约定：所有权和借用以在编译时防止数据竞争，`Result`/`?` 错误传播，库用 `thiserror` 应用用 `anyhow`，枚举和穷尽模式匹配使非法状态无法表示，trait 和泛型用于零成本抽象，通过 `Arc<Mutex<T>>`、通道和 async/await 实现安全并发，以及按领域组织的最小 `pub` 表面。

## 核心原则

### 1. 所有权和借用

Rust 的所有权系统在编译时防止数据竞争和内存 bug。

```rust
// 好：不需要所有权时传递引用
fn process(data: &[u8]) -> usize {
    data.len()
}

// 好：仅在需要存储或消费时获取所有权
fn store(data: Vec<u8>) -> Record {
    Record { payload: data }
}

// 坏：不必要地克隆以避免借用检查器
fn process_bad(data: &Vec<u8>) -> usize {
    let cloned = data.clone(); // 浪费 — 直接借用即可
    cloned.len()
}
```

### 使用 `Cow` 实现灵活所有权

```rust
use std::borrow::Cow;

fn normalize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))
    } else {
        Cow::Borrowed(input) // 不需要变更时零成本
    }
}
```

## 错误处理

### 使用 `Result` 和 `?` — 生产代码中绝不使用 `unwrap()`

```rust
// 好：用上下文传播错误
use anyhow::{Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config from {path}"))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("failed to parse config from {path}"))?;
    Ok(config)
}

// 坏：错误时 panic
fn load_config_bad(path: &str) -> Config {
    let content = std::fs::read_to_string(path).unwrap(); // Panic!
    toml::from_str(&content).unwrap()
}
```

### 库错误用 `thiserror`，应用错误用 `anyhow`

```rust
// 库代码：结构化、类型化错误
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("record not found: {id}")]
    NotFound { id: String },
    #[error("connection failed")]
    Connection(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

// 应用代码：灵活错误处理
use anyhow::{bail, Result};

fn run() -> Result<()> {
    let config = load_config("app.toml")?;
    if config.workers == 0 {
        bail!("worker count must be > 0");
    }
    Ok(())
}
```

### `Option` 组合器而非嵌套匹配

```rust
// 好：组合器链
fn find_user_email(users: &[User], id: u64) -> Option<String> {
    users.iter()
        .find(|u| u.id == id)
        .map(|u| u.email.clone())
}

// 坏：深度嵌套匹配
fn find_user_email_bad(users: &[User], id: u64) -> Option<String> {
    match users.iter().find(|u| u.id == id) {
        Some(user) => match &user.email {
            email => Some(email.clone()),
        },
        None => None,
    }
}
```

## 枚举和模式匹配

### 将状态建模为枚举

```rust
// 好：不可能的状态无法表示
enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected { session_id: String },
    Failed { reason: String, retries: u32 },
}

fn handle(state: &ConnectionState) {
    match state {
        ConnectionState::Disconnected => connect(),
        ConnectionState::Connecting { attempt } if *attempt > 3 => abort(),
        ConnectionState::Connecting { .. } => wait(),
        ConnectionState::Connected { session_id } => use_session(session_id),
        ConnectionState::Failed { retries, .. } if *retries < 5 => retry(),
        ConnectionState::Failed { reason, .. } => log_failure(reason),
    }
}
```

### 穷尽匹配 — 业务逻辑不用全匹配

```rust
// 好：显式处理每个变体
match command {
    Command::Start => start_service(),
    Command::Stop => stop_service(),
    Command::Restart => restart_service(),
    // 添加新变体会强制在此处理
}

// 坏：通配符隐藏新变体
match command {
    Command::Start => start_service(),
    _ => {} // 静默忽略 Stop、Restart 和未来变体
}
```

## Trait 和泛型

### 接受泛型，返回具体类型

```rust
// 好：泛型输入，具体输出
fn read_all(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

// 好：多约束的 trait bounds
fn process<T: Display + Send + 'static>(item: T) -> String {
    format!("processed: {item}")
}
```

### Trait 对象用于动态分发

```rust
// 当需要异构集合或插件系统时使用
trait Handler: Send + Sync {
    fn handle(&self, request: &Request) -> Response;
}

struct Router {
    handlers: Vec<Box<dyn Handler>>,
}

// 当需要性能时使用泛型（单态化）
fn fast_process<H: Handler>(handler: &H, request: &Request) -> Response {
    handler.handle(request)
}
```

### Newtype 模式用于类型安全

```rust
// 好：不同类型防止参数混淆
struct UserId(u64);
struct OrderId(u64);

fn get_order(user: UserId, order: OrderId) -> Result<Order> {
    // 不会意外交换 user 和 order ID
    todo!()
}

// 坏：容易交换参数
fn get_order_bad(user_id: u64, order_id: u64) -> Result<Order> {
    todo!()
}
```

## 结构体和数据建模

### 复杂构造的 Builder 模式

```rust
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: usize,
}

impl ServerConfig {
    fn builder(host: impl Into<String>, port: u16) -> ServerConfigBuilder {
        ServerConfigBuilder { host: host.into(), port, max_connections: 100 }
    }
}

struct ServerConfigBuilder { host: String, port: u16, max_connections: usize }

impl ServerConfigBuilder {
    fn max_connections(mut self, n: usize) -> Self { self.max_connections = n; self }
    fn build(self) -> ServerConfig {
        ServerConfig { host: self.host, port: self.port, max_connections: self.max_connections }
    }
}

// 使用：ServerConfig::builder("localhost", 8080).max_connections(200).build()
```

## 迭代器和闭包

### 优先使用迭代器链而非手动循环

```rust
// 好：声明式、惰性、可组合
let active_emails: Vec<String> = users.iter()
    .filter(|u| u.is_active)
    .map(|u| u.email.clone())
    .collect();

// 坏：命令式累积
let mut active_emails = Vec::new();
for user in &users {
    if user.is_active {
        active_emails.push(user.email.clone());
    }
}
```

### 使用类型注解的 `collect()`

```rust
// 收集到不同类型
let names: Vec<_> = items.iter().map(|i| &i.name).collect();
let lookup: HashMap<_, _> = items.iter().map(|i| (i.id, i)).collect();
let combined: String = parts.iter().copied().collect();

// 收集 Result — 短路首次错误
let parsed: Result<Vec<i32>, _> = strings.iter().map(|s| s.parse()).collect();
```

## 并发

### `Arc<Mutex<T>>` 用于共享可变状态

```rust
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));
let handles: Vec<_> = (0..10).map(|_| {
    let counter = Arc::clone(&counter);
    std::thread::spawn(move || {
        let mut num = counter.lock().expect("mutex poisoned");
        *num += 1;
    })
}).collect();

for handle in handles {
    handle.join().expect("worker thread panicked");
}
```

### 通道用于消息传递

```rust
use std::sync::mpsc;

let (tx, rx) = mpsc::sync_channel(16); // 带背压的有界通道

for i in 0..5 {
    let tx = tx.clone();
    std::thread::spawn(move || {
        tx.send(format!("message {i}")).expect("receiver disconnected");
    });
}
drop(tx); // 关闭发送者以便 rx 迭代器终止

for msg in rx {
    println!("{msg}");
}
```

### 使用 Tokio 的 Async

```rust
use tokio::time::Duration;

async fn fetch_with_timeout(url: &str) -> Result<String> {
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::get(url),
    )
    .await
    .context("request timed out")?
    .context("request failed")?;

    response.text().await.context("failed to read body")
}

// 生成并发任务
async fn fetch_all(urls: Vec<String>) -> Vec<Result<String>> {
    let handles: Vec<_> = urls.into_iter()
        .map(|url| tokio::spawn(async move {
            fetch_with_timeout(&url).await
        }))
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await.unwrap_or_else(|e| panic!("spawned task panicked: {e}")));
    }
    results
}
```

## Unsafe 代码

### Unsafe 可接受的情况

```rust
// 可接受：带有文档化不变量的 FFI 边界 (Rust 2024+)
/// # Safety
/// `ptr` 必须是有效的、对齐的指向已初始化 `Widget` 的指针。
unsafe fn widget_from_raw<'a>(ptr: *const Widget) -> &'a Widget {
    // SAFETY: 调用者保证 ptr 有效且对齐
    unsafe { &*ptr }
}

// 可接受：性能关键路径配合正确性证明
// SAFETY: 由于循环边界，index 始终 < len
unsafe { slice.get_unchecked(index) }
```

### Unsafe 不可接受的情况

```rust
// 坏：使用 unsafe 绕过借用检查器
// 坏：使用 unsafe 图方便
// 坏：使用 unsafe 但没有 Safety 注释
// 坏：在不相关的类型之间进行 transmute
```

## 模块系统和 Crate 结构

### 按领域组织，而非按类型

```text
my_app/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── auth/          # 领域模块
│   │   ├── mod.rs
│   │   ├── token.rs
│   │   └── middleware.rs
│   ├── orders/        # 领域模块
│   │   ├── mod.rs
│   │   ├── model.rs
│   │   └── service.rs
│   └── db/            # 基础设施
│       ├── mod.rs
│       └── pool.rs
├── tests/             # 集成测试
├── benches/           # 基准测试
└── Cargo.toml
```

### 可见性 — 最小化暴露

```rust
// 好：pub(crate) 用于内部共享
pub(crate) fn validate_input(input: &str) -> bool {
    !input.is_empty()
}

// 好：从 lib.rs 重导出公共 API
pub mod auth;
pub use auth::AuthMiddleware;

// 坏：让一切都 pub
pub fn internal_helper() {} // 应该是 pub(crate) 或 private
```

## 工具集成

### 基本命令

```bash
# 构建和检查
cargo build
cargo check              # 快速类型检查，不生成代码
cargo clippy             # Lint 和建议
cargo fmt                # 格式化代码

# 测试
cargo test
cargo test -- --nocapture    # 显示 println 输出
cargo test --lib             # 仅单元测试
cargo test --test integration # 仅集成测试

# 依赖
cargo audit              # 安全审计
cargo tree               # 依赖树
cargo update             # 更新依赖

# 性能
cargo bench              # 运行基准测试
```

## 快速参考：Rust 惯用语法

| 惯用语法 | 描述 |
|-------|-------------|
| 借用，不要克隆 | 除非需要所有权否则传 `&T` |
| 使非法状态无法表示 | 使用枚举仅建模有效状态 |
| 用 `?` 而非 `unwrap()` | 传播错误，库/生产代码永不 panic |
| 解析，不要验证 | 在边界将非结构化数据转换为类型化结构体 |
| Newtype 用于类型安全 | 将原语包装在 newtype 中防止参数交换 |
| 优先使用迭代器而非循环 | 声明式链更清晰且通常更快 |
| `#[must_use]` 用于 Result | 确保调用者处理返回值 |
| `Cow` 实现灵活所有权 | 借用足够时避免分配 |
| 穷尽匹配 | 业务关键枚举不用通配符 `_` |
| 最小 `pub` 表面 | 对内部 API 使用 `pub(crate)` |

## 应避免的反模式

```rust
// 坏：生产代码中 .unwrap()
let value = map.get("key").unwrap();

// 坏：不理解为什么就克隆以满足借用检查器
let data = expensive_data.clone();
process(&original, &data);

// 坏：能用 &str 时用 String
fn greet(name: String) { /* 应该是 &str */ }

// 坏：库中用 Box<dyn Error>（用 thiserror 代替）
fn parse(input: &str) -> Result<Data, Box<dyn std::error::Error>> { todo!() }

// 坏：忽略 must_use 警告
let _ = validate(input); // 静默丢弃 Result

// 坏：在 async 上下文中阻塞
async fn bad_async() {
    std::thread::sleep(Duration::from_secs(1)); // 阻塞执行器！
    // 使用：tokio::time::sleep(Duration::from_secs(1)).await;
}
```

**记住**：如果能编译，可能是正确的 — 但仅当你避免 `unwrap()`、最小化 `unsafe`，并让类型系统为你工作。

---

---
name: rust-testing
description: Rust 测试模式，包括单元测试、集成测试、异步测试、属性测试、mocking 和覆盖率。遵循 TDD 方法论。
origin: ECC
---

# Rust 测试模式

遵循 TDD 方法论的可靠、可维护测试的综合 Rust 测试模式。

## 激活时机

- 编写新的 Rust 函数、方法或 trait
- 为现有代码添加测试覆盖率
- 为性能关键代码创建基准测试
- 为输入验证实现属性测试
- 在 Rust 项目中遵循 TDD 工作流

## 工作原理

1. **识别目标代码** — 找到要测试的函数、trait 或模块
2. **写测试** — 在 `#[cfg(test)]` 模块中使用 `#[test]`，参数化测试用 rstest，属性测试用 proptest
3. **Mock 依赖** — 使用 mockall 隔离被测单元
4. **运行测试（RED）** — 验证测试因预期错误失败
5. **实现（GREEN）** — 写最小代码通过
6. **重构** — 保持测试绿色时改进
7. **检查覆盖率** — 使用 cargo-llvm-cov，目标 80%+

## Rust 的 TDD 工作流

### RED-GREEN-REFACTOR 循环

```
RED     → 先写一个失败的测试
GREEN   → 写最小代码使测试通过
REFACTOR → 保持测试绿色时改进代码
REPEAT  → 继续下一个需求
```

### Rust 中的分步 TDD

```rust
// RED：先写测试，用 todo!() 作为占位符
pub fn add(a: i32, b: i32) -> i32 { todo!() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add() { assert_eq!(add(2, 3), 5); }
}
// cargo test → 在 'not yet implemented' 处 panic
```

```rust
// GREEN：用最小实现替换 todo!()
pub fn add(a: i32, b: i32) -> i32 { a + b }
// cargo test → PASS，然后 REFACTOR 保持测试绿色
```

## 单元测试

### 模块级测试组织

```rust
// src/user.rs
pub struct User {
    pub name: String,
    pub email: String,
}

impl User {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Result<Self, String> {
        let email = email.into();
        if !email.contains('@') {
            return Err(format!("invalid email: {email}"));
        }
        Ok(Self { name: name.into(), email })
    }

    pub fn display_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_user_with_valid_email() {
        let user = User::new("Alice", "alice@example.com").unwrap();
        assert_eq!(user.display_name(), "Alice");
        assert_eq!(user.email, "alice@example.com");
    }

    #[test]
    fn rejects_invalid_email() {
        let result = User::new("Bob", "not-an-email");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid email"));
    }
}
```

### 断言宏

```rust
assert_eq!(2 + 2, 4);                                    // 相等
assert_ne!(2 + 2, 5);                                    // 不等
assert!(vec![1, 2, 3].contains(&2));                     // 布尔
assert_eq!(value, 42, "expected 42 but got {value}");    // 自定义消息
assert!((0.1_f64 + 0.2 - 0.3).abs() < f64::EPSILON);   // 浮点数比较
```

## 错误和 Panic 测试

### 测试 `Result` 返回

```rust
#[test]
fn parse_returns_error_for_invalid_input() {
    let result = parse_config("}{invalid");
    assert!(result.is_err());

    // 断言特定错误变体
    let err = result.unwrap_err();
    assert!(matches!(err, ConfigError::ParseError(_)));
}

#[test]
fn parse_succeeds_for_valid_input() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config(r#"{"port": 8080}"#)?;
    assert_eq!(config.port, 8080);
    Ok(()) // 如果任何 ? 返回 Err 则测试失败
}
```

### 测试 Panic

```rust
#[test]
#[should_panic]
fn panics_on_empty_input() {
    process(&[]);
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn panics_with_specific_message() {
    let v: Vec<i32> = vec![];
    let _ = v[0];
}
```

## 集成测试

### 文件结构

```text
my_crate/
├── src/
│   └── lib.rs
├── tests/              # 集成测试
│   ├── api_test.rs     # 每个文件是一个单独的测试二进制
│   ├── db_test.rs
│   └── common/         # 共享测试工具
│       └── mod.rs
```

### 编写集成测试

```rust
// tests/api_test.rs
use my_crate::{App, Config};

#[test]
fn full_request_lifecycle() {
    let config = Config::test_default();
    let app = App::new(config);

    let response = app.handle_request("/health");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, "OK");
}
```

## 异步测试

### 使用 Tokio

```rust
#[tokio::test]
async fn fetches_data_successfully() {
    let client = TestClient::new().await;
    let result = client.get("/data").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().items.len(), 3);
}

#[tokio::test]
async fn handles_timeout() {
    use std::time::Duration;
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        slow_operation(),
    ).await;

    assert!(result.is_err(), "should have timed out");
}
```

## 测试组织模式

### 使用 `rstest` 的参数化测试

```rust
use rstest::{rstest, fixture};

#[rstest]
#[case("hello", 5)]
#[case("", 0)]
#[case("rust", 4)]
fn test_string_length(#[case] input: &str, #[case] expected: usize) {
    assert_eq!(input.len(), expected);
}

// Fixtures
#[fixture]
fn test_db() -> TestDb {
    TestDb::new_in_memory()
}

#[rstest]
fn test_insert(test_db: TestDb) {
    test_db.insert("key", "value");
    assert_eq!(test_db.get("key"), Some("value".into()));
}
```

### 测试辅助函数

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 创建具有合理默认值的测试用户。
    fn make_user(name: &str) -> User {
        User::new(name, &format!("{name}@test.com")).unwrap()
    }

    #[test]
    fn user_display() {
        let user = make_user("alice");
        assert_eq!(user.display_name(), "alice");
    }
}
```

## 使用 `proptest` 的属性测试

### 基本属性测试

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn encode_decode_roundtrip(input in ".*") {
        let encoded = encode(&input);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(input, decoded);
    }

    #[test]
    fn sort_preserves_length(mut vec in prop::collection::vec(any::<i32>(), 0..100)) {
        let original_len = vec.len();
        vec.sort();
        assert_eq!(vec.len(), original_len);
    }

    #[test]
    fn sort_produces_ordered_output(mut vec in prop::collection::vec(any::<i32>(), 0..100)) {
        vec.sort();
        for window in vec.windows(2) {
            assert!(window[0] <= window[1]);
        }
    }
}
```

### 自定义策略

```rust
use proptest::prelude::*;

fn valid_email() -> impl Strategy<Value = String> {
    ("[a-z]{1,10}", "[a-z]{1,5}")
        .prop_map(|(user, domain)| format!("{user}@{domain}.com"))
}

proptest! {
    #[test]
    fn accepts_valid_emails(email in valid_email()) {
        assert!(User::new("Test", &email).is_ok());
    }
}
```

## 使用 `mockall` 的 Mocking

### 基于 Trait 的 Mocking

```rust
use mockall::{automock, predicate::eq};

#[automock]
trait UserRepository {
    fn find_by_id(&self, id: u64) -> Option<User>;
    fn save(&self, user: &User) -> Result<(), StorageError>;
}

#[test]
fn service_returns_user_when_found() {
    let mut mock = MockUserRepository::new();
    mock.expect_find_by_id()
        .with(eq(42))
        .times(1)
        .returning(|_| Some(User { id: 42, name: "Alice".into() }));

    let service = UserService::new(Box::new(mock));
    let user = service.get_user(42).unwrap();
    assert_eq!(user.name, "Alice");
}

#[test]
fn service_returns_none_when_not_found() {
    let mut mock = MockUserRepository::new();
    mock.expect_find_by_id()
        .returning(|_| None);

    let service = UserService::new(Box::new(mock));
    assert!(service.get_user(99).is_none());
}
```

## 文档测试

### 可执行文档

```rust
/// Adds two numbers together.
///
/// # Examples
///
/// ```
/// use my_crate::add;
///
/// assert_eq!(add(2, 3), 5);
/// assert_eq!(add(-1, 1), 0);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Parses a config string.
///
/// # Errors
///
/// Returns `Err` if the input is not valid TOML.
///
/// ```no_run
/// use my_crate::parse_config;
///
/// let config = parse_config(r#"port = 8080"#).unwrap();
/// assert_eq!(config.port, 8080);
/// ```
///
/// ```no_run
/// use my_crate::parse_config;
///
/// assert!(parse_config("}{invalid").is_err());
/// ```
pub fn parse_config(input: &str) -> Result<Config, ParseError> {
    todo!()
}
```

## 使用 Criterion 的基准测试

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "benchmark"
harness = false
```

```rust
// benches/benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn fibonacci(n: u64) -> u64 {
    match n {
        0 | 1 => n,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn bench_fibonacci(c: &mut Criterion) {
    c.bench_function("fib 20", |b| b.iter(|| fibonacci(black_box(20))));
}

criterion_group!(benches, bench_fibonacci);
criterion_main!(benches);
```

## 测试覆盖率

### 运行覆盖率

```bash
# 安装：cargo install cargo-llvm-cov（或在 CI 中使用 taiki-e/install-action）
cargo llvm-cov                    # 摘要
cargo llvm-cov --html             # HTML 报告
cargo llvm-cov --lcov > lcov.info # LCOV 格式用于 CI
cargo llvm-cov --fail-under-lines 80  # 低于阈值则失败
```

### 覆盖率目标

| 代码类型 | 目标 |
|-----------|--------|
| 关键业务逻辑 | 100% |
| 公共 API | 90%+ |
| 通用代码 | 80%+ |
| 生成/FFI 绑定 | 排除 |

## 测试命令

```bash
cargo test                        # 运行所有测试
cargo test -- --nocapture         # 显示 println 输出
cargo test test_name              # 运行匹配模式的测试
cargo test --lib                  # 仅单元测试
cargo test --test api_test        # 仅集成测试
cargo test --doc                  # 仅文档测试
cargo test --no-fail-fast         # 首次失败不停
cargo test -- --ignored           # 运行忽略的测试
```

## 最佳实践

**要做：**
- 先写测试（TDD）
- 使用 `#[cfg(test)]` 模块进行单元测试
- 测试行为，而非实现
- 使用描述性测试名称解释场景
- 优先使用 `assert_eq!` 以获得更好的错误消息
- 在返回 `Result` 的测试中使用 `?` 以获得更清晰的错误输出
- 保持测试独立 — 无共享可变状态

**不要：**
- 当可以测试 `Result::is_err()` 时使用 `#[should_panic]`
- Mock 一切 — 可行时优先使用集成测试
- 忽略 flaky 测试 — 修复或隔离它们
- 在测试中使用 `sleep()` — 使用通道、barrier 或 `tokio::time::pause()`
- 跳过错误路径测试

## CI 集成

```yaml
# GitHub Actions
test:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy, rustfmt

    - name: Check formatting
      run: cargo fmt --check

    - name: Clippy
      run: cargo clippy -- -D warnings

    - name: Run tests
      run: cargo test

    - uses: taiki-e/install-action@cargo-llvm-cov

    - name: Coverage
      run: cargo llvm-cov --fail-under-lines 80
```