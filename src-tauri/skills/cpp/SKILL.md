---
name: cpp-coding-standards
description: 基于 C++ Core Guidelines (isocpp.github.io) 的 C++ 编码标准。在编写、审查或重构 C++ 代码时使用，以强制执行现代、安全和惯用的实践。
origin: ECC
---

# C++ 编码标准（C++ Core Guidelines）

源自 [C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines) 的现代 C++（C++17/20/23）综合编码标准。强制类型安全、资源安全、不可变性和清晰性。

## 使用场景

- 编写新 C++ 代码（类、函数、模板）
- 审查或重构现有 C++ 代码
- 在 C++ 项目中做出架构决策
- 在 C++ 代码库中强制执行一致风格
- 在语言特性之间选择（例如 `enum` vs `enum class`、裸指针 vs 智能指针）

### 不使用场景

- 非 C++ 项目
- 无法采用现代 C++ 特性的遗留 C 代码库
- 嵌入式/裸金属上下文，其中特定指南与硬件约束冲突（选择性适配）

## 跨领域原则

这些主题贯穿整个指南并形成基础：

1. **到处 RAII**（P.8, R.1, E.6, CP.20）：将资源生命周期绑定到对象生命周期
2. **默认不可变**（P.10, Con.1-5, ES.25）：从 `const`/`constexpr` 开始；可变是例外
3. **类型安全**（P.4, I.4, ES.46-49, Enum.3）：使用类型系统在编译时防止错误
4. **表达意图**（P.3, F.1, NL.1-2, T.10）：名称、类型和概念应传达目的
5. **最小化复杂性**（F.2-3, ES.5, Per.4-5）：简单代码是正确的代码
6. **值语义优于指针语义**（C.10, R.3-5, F.20, CP.31）：优先返回值和作用域对象

## 哲学与接口（P.*, I.*）

### 关键规则

| 规则 | 摘要 |
| ------|---------|
| **P.1** | 直接在代码中表达想法 |
| **P.3** | 表达意图 |
| **P.4** | 理想情况下，程序应该是静态类型安全的 |
| **P.5** | 优先编译时检查而非运行时检查 |
| **P.8** | 不要泄漏任何资源 |
| **P.10** | 优先不可变数据而非可变数据 |
| **I.1** | 使接口显式 |
| **I.2** | 避免非 const 全局变量 |
| **I.4** | 使接口精确且强类型 |
| **I.11** | 绝不通过裸指针或引用转移所有权 |
| **I.23** | 保持函数参数数量低 |

### 要做

```cpp
// P.10 + I.4: 不可变、强类型接口
struct Temperature {
    double kelvin;
};

Temperature boil(const Temperature& water);
```

### 不要做

```cpp
// 弱接口：所有权不清晰、单位不清晰
double boil(double* temp);

// 非 const 全局变量
int g_counter = 0;  // I.2 违规
```

## 函数（F.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **F.1** | 将有意义操作打包为仔细命名的函数 |
| **F.2** | 函数应该执行单一逻辑操作 |
| **F.3** | 保持函数简短简单 |
| **F.4** | 如果函数可能在编译时求值，声明为 `constexpr` |
| **F.6** | 如果函数不能抛异常，声明为 `noexcept` |
| **F.8** | 优先纯函数 |
| **F.16** | 对于"输入"参数，便宜拷贝的类型按值传递，其他按 `const&` |
| **F.20** | 对于"输出"值，优先返回值而非输出参数 |
| **F.21** | 要返回多个"输出"值，优先返回结构体 |
| **F.43** | 绝不返回指向局部对象的指针或引用 |

### 参数传递

```cpp
// F.16: 便宜类型按值，其他按 const&
void print(int x);                           // 便宜：按值
void analyze(const std::string& data);       // 昂贵：按 const&
void transform(std::string s);               // sink：按值（会移动）

// F.20 + F.21: 返回值，而非输出参数
struct ParseResult {
    std::string token;
    int position;
};

ParseResult parse(std::string_view input);   // 好：返回结构体

// 坏：输出参数
void parse(std::string_view input,
           std::string& token, int& pos);    // 避免这样
```

### 纯函数和 constexpr

```cpp
// F.4 + F.8: 纯函数，尽可能 constexpr
constexpr int factorial(int n) noexcept {
    return (n <= 1) ? 1 : n * factorial(n - 1);
}

static_assert(factorial(5) == 120);
```

### 反模式

- 从函数返回 `T&&`（F.45）
- 使用 `va_arg` / C 风格可变参数（F.55）
- 在传递给其他线程的 lambda 中按引用捕获（F.53）
- 返回 `const T` 抑制移动语义（F.49）

## 类与类层次结构（C.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **C.2** | 如果存在不变量使用 `class`；如果数据成员独立变化使用 `struct` |
| **C.9** | 最小化成员暴露 |
| **C.20** | 如果可以避免定义默认操作，就不要定义（零规则） |
| **C.21** | 如果你定义或 `=delete` 任何复制/移动/析构函数，处理它们全部（五规则） |
| **C.35** | 基类析构函数：public virtual 或 protected non-virtual |
| **C.41** | 构造函数应该创建完全初始化的对象 |
| **C.46** | 将单参数构造函数声明为 `explicit` |
| **C.67** | 多态类应该抑制 public 复制/移动 |
| **C.128** | 虚函数：指定 `virtual`、`override` 或 `final` 中的恰好一个 |

### 零规则

```cpp
// C.20: 让编译器生成特殊成员
struct Employee {
    std::string name;
    std::string department;
    int id;
    // 不需要析构函数、复制/移动构造函数或赋值运算符
};
```

### 五规则

```cpp
// C.21: 如果必须管理资源，定义全部五个
class Buffer {
public:
    explicit Buffer(std::size_t size)
        : data_(std::make_unique<char[]>(size)), size_(size) {}

    ~Buffer() = default;

    Buffer(const Buffer& other)
        : data_(std::make_unique<char[]>(other.size_)), size_(other.size_) {
        std::copy_n(other.data_.get(), size_, data_.get());
    }

    Buffer& operator=(const Buffer& other) {
        if (this != &other) {
            auto new_data = std::make_unique<char[]>(other.size_);
            std::copy_n(other.data_.get(), other.size_, new_data.get());
            data_ = std::move(new_data);
            size_ = other.size_;
        }
        return *this;
    }

    Buffer(Buffer&&) noexcept = default;
    Buffer& operator=(Buffer&&) noexcept = default;

private:
    std::unique_ptr<char[]> data_;
    std::size_t size_;
};
```

### 类层次结构

```cpp
// C.35 + C.128: 虚析构函数，使用 override
class Shape {
public:
    virtual ~Shape() = default;
    virtual double area() const = 0;  // C.121: 纯接口
};

class Circle : public Shape {
public:
    explicit Circle(double r) : radius_(r) {}
    double area() const override { return 3.14159 * radius_ * radius_; }

private:
    double radius_;
};
```

### 反模式

- 在构造函数/析构函数中调用虚函数（C.82）
- 对非平凡类型使用 `memset`/`memcpy`（C.90）
- 为虚函数和覆盖器提供不同的默认参数（C.140）
- 使数据成员为 `const` 或引用，这会抑制移动/复制（C.12）

## 资源管理（R.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **R.1** | 使用 RAII 自动管理资源 |
| **R.3** | 裸指针（`T*`）是非拥有的 |
| **R.5** | 优先作用域对象；不必要地不要堆分配 |
| **R.10** | 避免 `malloc()`/`free()` |
| **R.11** | 避免显式调用 `new` 和 `delete` |
| **R.20** | 使用 `unique_ptr` 或 `shared_ptr` 表示所有权 |
| **R.21** | 优先 `unique_ptr` 除非需要共享所有权 |
| **R.22** | 使用 `make_shared()` 创建 `shared_ptr` |

### 智能指针使用

```cpp
// R.11 + R.20 + R.21: 带智能指针的 RAII
auto widget = std::make_unique<Widget>("config");  // 唯一所有权
auto cache  = std::make_shared<Cache>(1024);        // 共享所有权

// R.3: 裸指针 = 非拥有观察者
void render(const Widget* w) {  // 不拥有 w
    if (w) w->draw();
}

render(widget.get());
```

### RAII 模式

```cpp
// R.1: 资源获取即初始化
class FileHandle {
public:
    explicit FileHandle(const std::string& path)
        : handle_(std::fopen(path.c_str(), "r")) {
        if (!handle_) throw std::runtime_error("Failed to open: " + path);
    }

    ~FileHandle() {
        if (handle_) std::fclose(handle_);
    }

    FileHandle(const FileHandle&) = delete;
    FileHandle& operator=(const FileHandle&) = delete;
    FileHandle(FileHandle&& other) noexcept
        : handle_(std::exchange(other.handle_, nullptr)) {}
    FileHandle& operator=(FileHandle&& other) noexcept {
        if (this != &other) {
            if (handle_) std::fclose(handle_);
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

private:
    std::FILE* handle_;
};
```

### 反模式

- 裸 `new`/`delete`（R.11）
- C++ 代码中的 `malloc()`/`free()`（R.10）
- 单个表达式中多个资源分配（R.13 — 异常安全危害）
- `shared_ptr` 当 `unique_ptr` 足够时（R.21）

## 表达式与语句（ES.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **ES.5** | 保持作用域小 |
| **ES.20** | 始终初始化对象 |
| **ES.23** | 优先 `{}` 初始化器语法 |
| **ES.25** | 声明对象为 `const` 或 `constexpr` 除非打算修改 |
| **ES.28** | 对 `const` 变量的复杂初始化使用 lambda |
| **ES.45** | 避免魔法常量；使用符号常量 |
| **ES.46** | 避免窄化/丢失性算术转换 |
| **ES.47** | 使用 `nullptr` 而非 `0` 或 `NULL` |
| **ES.48** | 避免强制转换 |
| **ES.50** | 不要 cast 掉 `const` |

### 初始化

```cpp
// ES.20 + ES.23 + ES.25: 始终初始化，优先 {}，默认 const
const int max_retries{3};
const std::string name{"widget"};
const std::vector<int> primes{2, 3, 5, 7, 11};

// ES.28: Lambda 用于复杂的 const 初始化
const auto config = [&] {
    Config c;
    c.timeout = std::chrono::seconds{30};
    c.retries = max_retries;
    c.verbose = debug_mode;
    return c;
}();
```

### 反模式

- 未初始化变量（ES.20）
- 使用 `0` 或 `NULL` 作为指针（ES.47 — 使用 `nullptr`）
- C 风格强制转换（ES.48 — 使用 `static_cast`、`const_cast` 等）
- cast 掉 `const`（ES.50）
- 没有命名常量的魔法数字（ES.45）
- 混合有符号和无符号算术（ES.100）
- 在嵌套作用域中重用名称（ES.12）

## 错误处理（E.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **E.1** | 在设计早期开发错误处理策略 |
| **E.2** | 当函数无法完成其指定任务时抛出异常 |
| **E.6** | 使用 RAII 防止泄漏 |
| **E.12** | 当抛异常不可能或不可接受时使用 `noexcept` |
| **E.14** | 使用目的设计用户定义类型作为异常 |
| **E.15** | 按值抛出，按引用捕获 |
| **E.16** | 析构函数、释放和 swap 绝不能失败 |
| **E.17** | 不要尝试在每个函数中捕获每个异常 |

### 异常层次结构

```cpp
// E.14 + E.15: 自定义异常类型，按值抛，按引用捕获
class AppError : public std::runtime_error {
public:
    using std::runtime_error::runtime_error;
};

class NetworkError : public AppError {
public:
    NetworkError(const std::string& msg, int code)
        : AppError(msg), status_code(code) {}
    int status_code;
};

void fetch_data(const std::string& url) {
    // E.2: 抛异常以发信号失败
    throw NetworkError("connection refused", 503);
}

void run() {
    try {
        fetch_data("https://api.example.com");
    } catch (const NetworkError& e) {
        log_error(e.what(), e.status_code);
    } catch (const AppError& e) {
        log_error(e.what());
    }
    // E.17: 不要在这里捕获所有——让意外异常传播
}
```

### 反模式

- 抛出内置类型如 `int` 或字符串字面量（E.14）
- 按值捕获（切片风险）（E.15）
- 空 catch 块静默吞下错误
- 使用异常进行流控制（E.3）
- 基于 `errno` 等全局状态的错误处理（E.28）

## 常量与不可变性（Con.*）

### 所有规则

| 规则 | 摘要 |
|------|---------|
| **Con.1** | 默认情况下，使对象不可变 |
| **Con.2** | 默认情况下，使成员函数为 `const` |
| **Con.3** | 默认情况下，按 `const` 传递指针和引用 |
| **Con.4** | 对构造后不更改的值使用 `const` |
| **Con.5** | 对可在编译时计算的值使用 `constexpr` |

```cpp
// Con.1 到 Con.5: 默认不可变
class Sensor {
public:
    explicit Sensor(std::string id) : id_(std::move(id)) {}

    // Con.2: 默认 const 成员函数
    const std::string& id() const { return id_; }
    double last_reading() const { return reading_; }

    // 仅在需要修改时为非 const
    void record(double value) { reading_ = value; }

private:
    const std::string id_;  // Con.4: 构造后永不改变
    double reading_{0.0};
};

// Con.3: 按 const 引用传递
void display(const Sensor& s) {
    std::cout << s.id() << ": " << s.last_reading() << '\n';
}

// Con.5: 编译时常量
constexpr double PI = 3.14159265358979;
constexpr int MAX_SENSORS = 256;
```

## 并发与并行（CP.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **CP.2** | 避免数据竞争 |
| **CP.3** | 最小化可写数据的显式共享 |
| **CP.4** | 以任务而非线程的方式思考 |
| **CP.8** | 不要使用 `volatile` 进行同步 |
| **CP.20** | 使用 RAII，绝不要 plain `lock()`/`unlock()` |
| **CP.21** | 使用 `std::scoped_lock` 获取多个互斥锁 |
| **CP.22** | 持有锁时绝不调用未知代码 |
| **CP.42** | 不要在没有条件的情况下等待 |
| **CP.44** | 记住命名你的 `lock_guard`s 和 `unique_lock`s |
| **CP.100** | 除非绝对需要，否则不要使用无锁编程 |

### 安全锁定

```cpp
// CP.20 + CP.44: RAII 锁，始终命名
class ThreadSafeQueue {
public:
    void push(int value) {
        std::lock_guard<std::mutex> lock(mutex_);  // CP.44: 命名！
        queue_.push(value);
        cv_.notify_one();
    }

    int pop() {
        std::unique_lock<std::mutex> lock(mutex_);
        // CP.42: 始终在条件下等待
        cv_.wait(lock, [this] { return !queue_.empty(); });
        const int value = queue_.front();
        queue_.pop();
        return value;
    }

private:
    std::mutex mutex_;             // CP.50: 互斥锁与其数据在一起
    std::condition_variable cv_;
    std::queue<int> queue_;
};
```

### 多个互斥锁

```cpp
// CP.21: std::scoped_lock 用于多个互斥锁（无死锁）
void transfer(Account& from, Account& to, double amount) {
    std::scoped_lock lock(from.mutex_, to.mutex_);
    from.balance_ -= amount;
    to.balance_ += amount;
}
```

### 反模式

- 用于同步的 `volatile`（CP.8 — 它仅用于硬件 I/O）
- 分离线程（CP.26 — 生命周期管理几乎不可能）
- 未命名的锁守卫：`std::lock_guard<std::mutex>(m);` 立即销毁（CP.44）
- 持有锁时调用回调（CP.22 — 死锁风险）
- 无深入专业知识的无锁编程（CP.100）

## 模板与泛型编程（T.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **T.1** | 使用模板提升抽象级别 |
| **T.2** | 使用模板表达多参数类型的算法 |
| **T.10** | 为所有模板参数指定概念 |
| **T.11** | 尽可能使用标准概念 |
| **T.13** | 对简单概念使用简写表示法 |
| **T.43** | 优先 `using` 而非 `typedef` |
| **T.120** | 仅在真正需要时使用模板元编程 |
| **T.144** | 不要特化函数模板（重载代替） |

### 概念（C++20）

```cpp
#include <concepts>

// T.10 + T.11: 用标准概念约束模板
template<std::integral T>
T gcd(T a, T b) {
    while (b != 0) {
        a = std::exchange(b, a % b);
    }
    return a;
}

// T.13: 简写概念语法
void sort(std::ranges::random_access_range auto& range) {
    std::ranges::sort(range);
}

// 特定领域约束的自定义概念
template<typename T>
concept Serializable = requires(const T& t) {
    { t.serialize() } -> std::convertible_to<std::string>;
};

template<Serializable T>
void save(const T& obj, const std::string& path);
```

### 反模式

- 可见命名空间中的无约束模板（T.47）
- 特化函数模板而非重载（T.144）
- 当 `constexpr` 足够时使用模板元编程（T.120）
- 使用 `typedef` 而非 `using`（T.43）

## 标准库（SL.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **SL.1** | 尽可能使用库 |
| **SL.2** | 优先标准库而非其他库 |
| **SL.con.1** | 优先 `std::array` 或 `std::vector` 而非 C 数组 |
| **SL.con.2** | 默认优先 `std::vector` |
| **SL.str.1** | 使用 `std::string` 拥有字符序列 |
| **SL.str.2** | 使用 `std::string_view` 引用字符序列 |
| **SL.io.50** | 避免 `endl`（使用 `'\n'` — `endl` 强制刷新） |

```cpp
// SL.con.1 + SL.con.2: 优先 vector/array 而非 C 数组
const std::array<int, 4> fixed_data{1, 2, 3, 4};
std::vector<std::string> dynamic_data;

// SL.str.1 + SL.str.2: string 拥有，string_view 观察
std::string build_greeting(std::string_view name) {
    return "Hello, " + std::string(name) + "!";
}

// SL.io.50: 使用 '\n' 而非 endl
std::cout << "result: " << value << '\n';
```

## 枚举（Enum.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **Enum.1** | 优先枚举而非宏 |
| **Enum.3** | 优先 `enum class` 而非 plain `enum` |
| **Enum.5** | 不要对枚举器使用 ALL_CAPS |
| **Enum.6** | 避免未命名的枚举 |

```cpp
// Enum.3 + Enum.5: 作用域枚举，不用 ALL_CAPS
enum class Color { red, green, blue };
enum class LogLevel { debug, info, warning, error };

// 坏：plain enum 泄漏名称，ALL_CAPS 与宏冲突
enum { RED, GREEN, BLUE };           // Enum.3 + Enum.5 + Enum.6 违规
#define MAX_SIZE 100                  // Enum.1 违规 — 使用 constexpr
```

## 源文件与命名（SF.*, NL.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **SF.1** | 对代码文件使用 `.cpp`，对接口文件使用 `.h` |
| **SF.7** | 不要在头文件全局作用域写 `using namespace` |
| **SF.8** | 对所有 `.h` 文件使用 `#include` 守卫 |
| **SF.11** | 头文件应该自包含 |
| **NL.5** | 避免在名称中编码类型信息（不要用匈牙利命名法） |
| **NL.8** | 使用一致的命名风格 |
| **NL.9** | 仅对宏名称使用 ALL_CAPS |
| **NL.10** | 优先 `underscore_style` 名称 |

### 头文件守卫

```cpp
// SF.8: Include 守卫（或 #pragma once）
#ifndef PROJECT_MODULE_WIDGET_H
#define PROJECT_MODULE_WIDGET_H

// SF.11: 自包含 — 包含此头文件需要的所有内容
#include <string>
#include <vector>

namespace project::module {

class Widget {
public:
    explicit Widget(std::string name);
    const std::string& name() const;

private:
    std::string name_;
};

}  // namespace project::module

#endif  // PROJECT_MODULE_WIDGET_H
```

### 命名约定

```cpp
// NL.8 + NL.10: 一致的 underscore_style
namespace my_project {

constexpr int max_buffer_size = 4096;  // NL.9: 不是 ALL_CAPS（它不是宏）

class tcp_connection {                 // underscore_style 类
public:
    void send_message(std::string_view msg);
    bool is_connected() const;

private:
    std::string host_;                 // 成员用尾部下划线
    int port_;
};

}  // namespace my_project
```

### 反模式

- 在头文件全局作用域 `using namespace std;`（SF.7）
- 依赖包含顺序的头文件（SF.10, SF.11）
- 匈牙利命名法如 `strName`、`iCount`（NL.5）
- 除宏外对任何东西使用 ALL_CAPS（NL.9）

## 性能（Per.*）

### 关键规则

| 规则 | 摘要 |
|------|---------|
| **Per.1** | 不要无理由地优化 |
| **Per.2** | 不要过早优化 |
| **Per.6** | 不要在没有测量的情况下声称性能 |
| **Per.7** | 设计以启用优化 |
| **Per.10** | 依赖静态类型系统 |
| **Per.11** | 将计算从运行时移到编译时 |
| **Per.19** | 可预测地访问内存 |

### 指南

```cpp
// Per.11: 尽可能使用编译时计算
constexpr auto lookup_table = [] {
    std::array<int, 256> table{};
    for (int i = 0; i < 256; ++i) {
        table[i] = i * i;
    }
    return table;
}();

// Per.19: 优先连续数据以提高缓存友好性
std::vector<Point> points;           // 好：连续
std::vector<std::unique_ptr<Point>> indirect_points; // 坏：指针追逐
```

### 反模式

- 在没有分析数据的情况下优化（Per.1, Per.6）
- 选择"聪明的"底层代码而非清晰的抽象（Per.4, Per.5）
- 忽略数据布局和缓存行为（Per.19）

## 快速参考检查清单

在标记 C++ 工作完成前：

- [ ] 无裸 `new`/`delete` — 使用智能指针或 RAII（R.11）
- [ ] 对象在声明时初始化（ES.20）
- [ ] 变量默认 `const`/`constexpr`（Con.1, ES.25）
- [ ] 成员函数在可能时为 `const`（Con.2）
- [ ] `enum class` 而非 plain `enum`（Enum.3）
- [ ] `nullptr` 而非 `0`/`NULL`（ES.47）
- [ ] 无窄化转换（ES.46）
- [ ] 无 C 风格强制转换（ES.48）
- [ ] 单参数构造函数为 `explicit`（C.46）
- [ ] 应用零规则或五规则（C.20, C.21）
- [ ] 基类析构函数为 public virtual 或 protected non-virtual（C.35）
- [ ] 模板用概念约束（T.10）
- [ ] 头文件中全局作用域无 `using namespace`（SF.7）
- [ ] 头文件有 include 守卫且自包含（SF.8, SF.11）
- [ ] 锁使用 RAII（`scoped_lock`/`lock_guard`）（CP.20）
- [ ] 异常为自定义类型，按值抛，按引用捕获（E.14, E.15）
- [ ] 使用 `'\n'` 而非 `std::endl`（SL.io.50）
- [ ] 无魔法数字（ES.45）

---

---
name: cpp-testing
description: 仅在编写/更新/修复 C++ 测试、配置 GoogleTest/CTest、诊断失败或不稳定的测试、或添加覆盖率/ sanitizer 时使用。
origin: ECC
---

# C++ 测试（智能体技能）

面向智能体的现代 C++（C++17/20）测试工作流，使用 GoogleTest/GoogleMock 和 CMake/CTest。

## 使用场景

- 编写新的 C++ 测试或修复现有测试
- 为 C++ 组件设计单元/集成测试覆盖率
- 添加测试覆盖率、CI 门控或回归保护
- 配置 CMake/CTest 工作流以实现一致执行
- 调查测试失败或不稳定行为
- 启用 sanitizer 以进行内存/竞态诊断

### 不使用场景

- 在没有测试更改的情况下实现新产品功能
- 与测试覆盖率或失败无关的大规模重构
- 在没有测试回归要验证的情况下进行性能调优
- 非 C++ 项目或非测试任务

## 核心概念

- **TDD 循环**：red → green → refactor（测试优先、最小修复、然后清理）。
- **隔离**：优先依赖注入和 fake 而非全局状态。
- **测试布局**：`tests/unit`、`tests/integration`、`tests/testdata`。
- **Mock vs fake**：mock 用于交互，fake 用于状态行为。
- **CTest 发现**：使用 `gtest_discover_tests()` 以实现稳定测试发现。
- **CI 信号**：先运行子集，然后运行完整套件 `--output-on-failure`。

## TDD 工作流

遵循 RED → GREEN → REFACTOR 循环：

1. **RED**：编写捕获新行为的失败测试
2. **GREEN**：实现最小的更改以通过
3. **REFACTOR**：在测试保持绿色时清理

```cpp
// tests/add_test.cpp
#include <gtest/gtest.h>

int Add(int a, int b); // 由生产代码提供。

TEST(AddTest, AddsTwoNumbers) { // RED
  EXPECT_EQ(Add(2, 3), 5);
}

// src/add.cpp
int Add(int a, int b) { // GREEN
  return a + b;
}

// REFACTOR: 一旦测试通过就简化/重命名
```

## 代码示例

### 基本单元测试（gtest）

```cpp
// tests/calculator_test.cpp
#include <gtest/gtest.h>

int Add(int a, int b); // 由生产代码提供。

TEST(CalculatorTest, AddsTwoNumbers) {
    EXPECT_EQ(Add(2, 3), 5);
}
```

### Fixture（gtest）

```cpp
// tests/user_store_test.cpp
// 伪代码存根：替换为项目类型 UserStore/User。
#include <gtest/gtest.h>
#include <memory>
#include <optional>
#include <string>

struct User { std::string name; };
class UserStore {
public:
    explicit UserStore(std::string /*path*/) {}
    void Seed(std::initializer_list<User> /*users*/) {}
    std::optional<User> Find(const std::string &/*name*/) { return User{"alice"}; }
};

class UserStoreTest : public ::testing::Test {
protected:
    void SetUp() override {
        store = std::make_unique<UserStore>(":memory:");
        store->Seed({{"alice"}, {"bob"}});
    }

    std::unique_ptr<UserStore> store;
};

TEST_F(UserStoreTest, FindsExistingUser) {
    auto user = store->Find("alice");
    ASSERT_TRUE(user.has_value());
    EXPECT_EQ(user->name, "alice");
}
```

### Mock（gmock）

```cpp
// tests/notifier_test.cpp
#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include <string>

class Notifier {
public:
    virtual ~Notifier() = default;
    virtual void Send(const std::string &message) = 0;
};

class MockNotifier : public Notifier {
public:
    MOCK_METHOD(void, Send, (const std::string &message), (override));
};

class Service {
public:
    explicit Service(Notifier &notifier) : notifier_(notifier) {}
    void Publish(const std::string &message) { notifier_.Send(message); }

private:
    Notifier &notifier_;
};

TEST(ServiceTest, SendsNotifications) {
    MockNotifier notifier;
    Service service(notifier);

    EXPECT_CALL(notifier, Send("hello")).Times(1);
    service.Publish("hello");
}
```

### CMake/CTest 快速入门

```cmake
# CMakeLists.txt（摘录）
cmake_minimum_required(VERSION 3.20)
project(example LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include(FetchContent)
# 优先项目锁定版本。如果使用 tag，按项目策略使用固定版本。
set(GTEST_VERSION v1.17.0) # 按项目策略调整。
FetchContent_Declare(
  googletest
  # Google Test 框架（官方仓库）
  URL https://github.com/google/googletest/archive/refs/tags/${GTEST_VERSION}.zip
)
FetchContent_MakeAvailable(googletest)

add_executable(example_tests
  tests/calculator_test.cpp
  src/calculator.cpp
)
target_link_libraries(example_tests GTest::gtest GTest::gmock GTest::gtest_main)

enable_testing()
include(GoogleTest)
gtest_discover_tests(example_tests)
```

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build -j
ctest --test-dir build --output-on-failure
```

## 运行测试

```bash
ctest --test-dir build --output-on-failure
ctest --test-dir build -R ClampTest
ctest --test-dir build -R "UserStoreTest.*" --output-on-failure
```

```bash
./build/example_tests --gtest_filter=ClampTest.*
./build/example_tests --gtest_filter=UserStoreTest.FindsExistingUser
```

## 调试失败

1. 用 gtest 过滤器重新运行单个失败测试。
2. 在失败断言周围添加作用域日志。
3. 用 sanitizer 重新运行。
4. 一旦根本原因修复，扩展到完整套件。

## 覆盖率

优先目标级设置而非全局标志。

```cmake
option(ENABLE_COVERAGE "Enable coverage flags" OFF)

if(ENABLE_COVERAGE)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU")
    target_compile_options(example_tests PRIVATE --coverage)
    target_link_options(example_tests PRIVATE --coverage)
  elseif(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    target_compile_options(example_tests PRIVATE -fprofile-instr-generate -fcoverage-mapping)
    target_link_options(example_tests PRIVATE -fprofile-instr-generate)
  endif()
endif()
```

GCC + gcov + lcov:

```bash
cmake -S . -B build-cov -DENABLE_COVERAGE=ON
cmake --build build-cov -j
ctest --test-dir build-cov
lcov --capture --directory build-cov --output-file coverage.info
lcov --remove coverage.info '/usr/*' --output-file coverage.info
genhtml coverage.info --output-directory coverage
```

Clang + llvm-cov:

```bash
cmake -S . -B build-llvm -DENABLE_COVERAGE=ON -DCMAKE_CXX_COMPILER=clang++
cmake --build build-llvm -j
LLVM_PROFILE_FILE="build-llvm/default.profraw" ctest --test-dir build-llvm
llvm-profdata merge -sparse build-llvm/default.profraw -o build-llvm/default.profdata
llvm-cov report build-llvm/example_tests -instr-profile=build-llvm/default.profdata
```

## Sanitizer

```cmake
option(ENABLE_ASAN "Enable AddressSanitizer" OFF)
option(ENABLE_UBSAN "Enable UndefinedBehaviorSanitizer" OFF)
option(ENABLE_TSAN "Enable ThreadSanitizer" OFF)

if(ENABLE_ASAN)
  add_compile_options(-fsanitize=address -fno-omit-frame-pointer)
  add_link_options(-fsanitize=address)
endif()
if(ENABLE_UBSAN)
  add_compile_options(-fsanitize=undefined -fno-omit-frame-pointer)
  add_link_options(-fsanitize=undefined)
endif()
if(ENABLE_TSAN)
  add_compile_options(-fsanitize=thread)
  add_link_options(-fsanitize=thread)
endif()
```

## 不稳定测试护栏

- 绝不要使用 `sleep` 进行同步；使用条件变量或 latch。
- 使临时目录对每个测试唯一并始终清理。
- 避免在单元测试中进行实时、网络或文件系统依赖。
- 对随机输入使用确定性种子。

## 最佳实践

### 要做

- 保持测试确定性和隔离
- 优先依赖注入而非全局
- 对前置条件使用 `ASSERT_*`，对多个检查使用 `EXPECT_*`
- 在 CTest 标签或目录中分离单元测试与集成测试
- 在 CI 中启用 sanitizer 以进行内存和竞态检测

### 不要做

- 不要在单元测试中依赖实时或网络
- 当可以使用条件变量时不要使用 sleep 进行同步
- 不要过度 mock 简单值对象
- 不要对非关键日志使用脆弱的字符串匹配

### 常见陷阱

- **使用固定临时路径** → 为每个测试生成唯一临时目录并清理。
- **依赖墙上时钟时间** → 注入时钟或使用假时间源。
- **不稳定的并发测试** → 使用条件变量/latch 和有界等待。
- **隐藏的全局状态** → 在 fixture 中重置全局状态或移除全局。
- **过度 mock** → 对状态行为优先使用 fake，仅 mock 交互。
- **缺少 sanitizer 运行** → 在 CI 中添加 ASan/UBSan/TSan 构建。
- **仅在调试构建上覆盖** → 确保覆盖目标使用一致的标志。

## 可选附录：模糊测试/属性测试

仅在项目已支持 LLVM/libFuzzer 或属性测试库时使用。

- **libFuzzer**：最适合最小 I/O 的纯函数。
- **RapidCheck**：属性测试以验证不变量。

最小 libFuzzer 工具（伪代码：替换 ParseConfig）：

```cpp
#include <cstddef>
#include <cstdint>
#include <string>

extern "C" int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    std::string input(reinterpret_cast<const char *>(data), size);
    // ParseConfig(input); // 项目函数
    return 0;
}
```

## GoogleTest 的替代方案

- **Catch2**：header-only，富有表现力的 matcher
- **doctest**：轻量级，最小编译开销