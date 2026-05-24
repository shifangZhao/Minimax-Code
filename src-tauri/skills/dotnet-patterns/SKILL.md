---
name: dotnet-patterns
description: 惯用 C# 和 .NET 模式、约定、依赖注入、async/await 以及构建健壮、可维护 .NET 应用程序的最佳实践。
origin: ECC
---

# .NET 开发模式

用于构建健壮、高性能和可维护应用程序的惯用 C# 和 .NET 模式。

## 激活时机

- 编写新的 C# 代码
- 审查 C# 代码
- 重构现有 .NET 应用程序
- 使用 ASP.NET Core 设计服务架构

## 核心原则

### 1. 优先不可变性

对数据模型使用记录和 init-only 属性。可变性应该是明确的、有据可查的选择。

```csharp
// 好：不可变值对象
public sealed record Money(decimal Amount, string Currency);

// 好：带 init setter 的不可变 DTO
public sealed class CreateOrderRequest
{
    public required string CustomerId { get; init; }
    public required IReadOnlyList<OrderItem> Items { get; init; }
}

// 坏：带公共 setter 的可变模型
public class Order
{
    public string CustomerId { get; set; }
    public List<OrderItem> Items { get; set; }
}
```

### 2. 显式优于隐式

对可空性、访问修饰符和意图要清晰。

```csharp
// 好：显式访问修饰符和可空性
public sealed class UserService
{
    private readonly IUserRepository _repository;
    private readonly ILogger<UserService> _logger;

    public UserService(IUserRepository repository, ILogger<UserService> logger)
    {
        _repository = repository ?? throw new ArgumentNullException(nameof(repository));
        _logger = logger ?? throw new ArgumentNullException(nameof(logger));
    }

    public async Task<User?> FindByIdAsync(Guid id, CancellationToken cancellationToken)
    {
        return await _repository.FindByIdAsync(id, cancellationToken);
    }
}
```

### 3. 依赖抽象

对服务边界使用接口。通过 DI 容器注册。

```csharp
// 好：基于接口的依赖
public interface IOrderRepository
{
    Task<Order?> FindByIdAsync(Guid id, CancellationToken cancellationToken);
    Task<IReadOnlyList<Order>> FindByCustomerAsync(string customerId, CancellationToken cancellationToken);
    Task AddAsync(Order order, CancellationToken cancellationToken);
}

// 注册
builder.Services.AddScoped<IOrderRepository, SqlOrderRepository>();
```

## Async/Await 模式

### 正确的 Async 使用

```csharp
// 好：全程 async，带 CancellationToken
public async Task<OrderSummary> GetOrderSummaryAsync(
    Guid orderId,
    CancellationToken cancellationToken)
{
    var order = await _repository.FindByIdAsync(orderId, cancellationToken)
        ?? throw new NotFoundException($"Order {orderId} not found");

    var customer = await _customerService.GetAsync(order.CustomerId, cancellationToken);

    return new OrderSummary(order, customer);
}

// 坏：阻塞 async
public OrderSummary GetOrderSummary(Guid orderId)
{
    var order = _repository.FindByIdAsync(orderId, CancellationToken.None).Result; // 死锁风险
    return new OrderSummary(order);
}
```

### 并行 Async 操作

```csharp
// 好：并发独立操作
public async Task<DashboardData> LoadDashboardAsync(CancellationToken cancellationToken)
{
    var ordersTask = _orderService.GetRecentAsync(cancellationToken);
    var metricsTask = _metricsService.GetCurrentAsync(cancellationToken);
    var alertsTask = _alertService.GetActiveAsync(cancellationToken);

    await Task.WhenAll(ordersTask, metricsTask, alertsTask);

    return new DashboardData(
        Orders: await ordersTask,
        Metrics: await metricsTask,
        Alerts: await alertsTask);
}
```

## Options 模式

将配置节绑定到强类型对象。

```csharp
public sealed class SmtpOptions
{
    public const string SectionName = "Smtp";

    public required string Host { get; init; }
    public required int Port { get; init; }
    public required string Username { get; init; }
    public bool UseSsl { get; init; } = true;
}

// 注册
builder.Services.Configure<SmtpOptions>(
    builder.Configuration.GetSection(SmtpOptions.SectionName));

// 通过注入使用
public class EmailService(IOptions<SmtpOptions> options)
{
    private readonly SmtpOptions _smtp = options.Value;
}
```

## Result 模式

对预期失败返回显式成功/失败，而非抛出异常。

```csharp
public sealed record Result<T>
{
    public bool IsSuccess { get; }
    public T? Value { get; }
    public string? Error { get; }

    private Result(T value) { IsSuccess = true; Value = value; }
    private Result(string error) { IsSuccess = false; Error = error; }

    public static Result<T> Success(T value) => new(value);
    public static Result<T> Failure(string error) => new(error);
}

// 使用
public async Task<Result<Order>> PlaceOrderAsync(CreateOrderRequest request)
{
    if (request.Items.Count == 0)
        return Result<Order>.Failure("Order must contain at least one item");

    var order = Order.Create(request);
    await _repository.AddAsync(order, CancellationToken.None);
    return Result<Order>.Success(order);
}
```

## 带 EF Core 的 Repository 模式

```csharp
public sealed class SqlOrderRepository : IOrderRepository
{
    private readonly AppDbContext _db;

    public SqlOrderRepository(AppDbContext db) => _db = db;

    public async Task<Order?> FindByIdAsync(Guid id, CancellationToken cancellationToken)
    {
        return await _db.Orders
            .Include(o => o.Items)
            .AsNoTracking()
            .FirstOrDefaultAsync(o => o.Id == id, cancellationToken);
    }

    public async Task<IReadOnlyList<Order>> FindByCustomerAsync(
        string customerId,
        CancellationToken cancellationToken)
    {
        return await _db.Orders
            .Where(o => o.CustomerId == customerId)
            .OrderByDescending(o => o.CreatedAt)
            .AsNoTracking()
            .ToListAsync(cancellationToken);
    }

    public async Task AddAsync(Order order, CancellationToken cancellationToken)
    {
        _db.Orders.Add(order);
        await _db.SaveChangesAsync(cancellationToken);
    }
}
```

## 中间件和管道

```csharp
// 自定义中间件
public sealed class RequestTimingMiddleware
{
    private readonly RequestDelegate _next;
    private readonly ILogger<RequestTimingMiddleware> _logger;

    public RequestTimingMiddleware(RequestDelegate next, ILogger<RequestTimingMiddleware> logger)
    {
        _next = next;
        _logger = logger;
    }

    public async Task InvokeAsync(HttpContext context)
    {
        var stopwatch = Stopwatch.StartNew();
        try
        {
            await _next(context);
        }
        finally
        {
            stopwatch.Stop();
            _logger.LogInformation(
                "Request {Method} {Path} completed in {ElapsedMs}ms with status {StatusCode}",
                context.Request.Method,
                context.Request.Path,
                stopwatch.ElapsedMilliseconds,
                context.Response.StatusCode);
        }
    }
}
```

## Minimal API 模式

```csharp
// 使用路由组组织
var orders = app.MapGroup("/api/orders")
    .RequireAuthorization()
    .WithTags("Orders");

orders.MapGet("/{id:guid}", async (
    Guid id,
    IOrderRepository repository,
    CancellationToken cancellationToken) =>
{
    var order = await repository.FindByIdAsync(id, cancellationToken);
    return order is not null
        ? TypedResults.Ok(order)
        : TypedResults.NotFound();
});

orders.MapPost("/", async (
    CreateOrderRequest request,
    IOrderService service,
    CancellationToken cancellationToken) =>
{
    var result = await service.PlaceOrderAsync(request, cancellationToken);
    return result.IsSuccess
        ? TypedResults.Created($"/api/orders/{result.Value!.Id}", result.Value)
        : TypedResults.BadRequest(result.Error);
});
```

## Guard 子句

```csharp
// 好：带清晰验证的提前返回
public async Task<ProcessResult> ProcessPaymentAsync(
    PaymentRequest request,
    CancellationToken cancellationToken)
{
    ArgumentNullException.ThrowIfNull(request);

    if (request.Amount <= 0)
        throw new ArgumentOutOfRangeException(nameof(request.Amount), "Amount must be positive");

    if (string.IsNullOrWhiteSpace(request.Currency))
        throw new ArgumentException("Currency is required", nameof(request.Currency));

    // 快乐路径继续在这里，没有嵌套
    var gateway = _gatewayFactory.Create(request.Currency);
    return await gateway.ChargeAsync(request, cancellationToken);
}
```

## 应避免的反模式

| 反模式 | 修复 |
|---|---|
| `async void` 方法 | 返回 `Task`（事件处理器除外） |
| `.Result` 或 `.Wait()` | 使用 `await` |
| `catch (Exception) { }` | 处理或带上下文重新抛出 |
| 构造函数中的 `new Service()` | 使用构造函数注入 |
| `public` 字段 | 使用具有适当访问器的属性 |
| 业务逻辑中的 `dynamic` | 使用泛型或显式类型 |
| 可变 `static` 状态 | 使用 DI 作用域或 `ConcurrentDictionary` |
| 循环中的 `string.Format` | 使用 `StringBuilder` 或内插字符串处理器 |