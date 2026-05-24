---
name: laravel-patterns
description: Laravel 架构模式、路由/控制器、Eloquent ORM、服务层、队列、事件、缓存和 API 资源，用于生产应用。
origin: ECC
---

# Laravel 开发模式

用于可扩展、可维护应用的生产级 Laravel 架构模式。

## 何时使用

- 构建 Laravel Web 应用或 API
- 构建控制器、服务和领域逻辑的清晰边界
- 使用 Eloquent 模型和关系
- 使用资源和分页设计 API
- 添加队列、事件、缓存和后台作业

## 工作原理

- 在清晰边界周围构建应用（控制器 -> 服务/动作 -> 模型）。
- 使用显式绑定和作用域绑定保持路由可预测；仍然强制执行授权进行访问控制。
- 优先使用类型化模型、casts 和 scopes 保持领域逻辑一致。
- 将 IO 重度工作保持在队列和缓存昂贵读取中。
- 在 `config/*` 中集中配置并保持环境显式。

## 示例

### 项目结构

使用清晰的层边界（HTTP、服务/动作、模型）的常规 Laravel 布局。

### 推荐布局

```
app/
├── Actions/            # 单一用途用例
├── Console/
├── Events/
├── Exceptions/
├── Http/
│   ├── Controllers/
│   ├── Middleware/
│   ├── Requests/       # 表单请求验证
│   └── Resources/      # API 资源
├── Jobs/
├── Models/
├── Policies/
├── Providers/
├── Services/           # 协调领域服务
└── Support/
config/
database/
├── factories/
├── migrations/
└── seeders/
resources/
├── views/
└── lang/
routes/
├── api.php
├── web.php
└── console.php
```

### 控制器 -> 服务 -> 动作

保持控制器薄。将编排放在服务中，单一用途逻辑放在动作中。

```php
final class CreateOrderAction
{
    public function __construct(private OrderRepository $orders) {}

    public function handle(CreateOrderData $data): Order
    {
        return $this->orders->create($data);
    }
}

final class OrdersController extends Controller
{
    public function __construct(private CreateOrderAction $createOrder) {}

    public function store(StoreOrderRequest $request): JsonResponse
    {
        $order = $this->createOrder->handle($request->toDto());

        return response()->json([
            'success' => true,
            'data' => OrderResource::make($order),
            'error' => null,
            'meta' => null,
        ], 201);
    }
}
```

### 路由和控制器

优先路由模型绑定和资源控制器以保持清晰。

```php
use Illuminate\Support\Facades\Route;

Route::middleware('auth:sanctum')->group(function () {
    Route::apiResource('projects', ProjectController::class);
});
```

### 路由模型绑定（作用域）

使用作用域绑定防止跨租户访问。

```php
Route::scopeBindings()->group(function () {
    Route::get('/accounts/{account}/projects/{project}', [ProjectController::class, 'show']);
});
```

### 嵌套路由和绑定名称

- 保持前缀和路径一致以避免双重嵌套（如 `conversation` vs `conversations`）。
- 使用匹配绑定模型的单个参数名（如 `{conversation}` 对应 `Conversation`）。
- 嵌套时优先作用域绑定以强制父子关系。

```php
use App\Http\Controllers\Api\ConversationController;
use App\Http\Controllers\Api\MessageController;
use Illuminate\Support\Facades\Route;

Route::middleware('auth:sanctum')->prefix('conversations')->group(function () {
    Route::post('/', [ConversationController::class, 'store'])->name('conversations.store');

    Route::scopeBindings()->group(function () {
        Route::get('/{conversation}', [ConversationController::class, 'show'])
            ->name('conversations.show');

        Route::post('/{conversation}/messages', [MessageController::class, 'store'])
            ->name('conversation-messages.store');

        Route::get('/{conversation}/messages/{message}', [MessageController::class, 'show'])
            ->name('conversation-messages.show');
    });
});
```

如果希望参数解析为不同的模型类，定义显式绑定。对于自定义绑定逻辑，使用 `Route::bind()` 或在模型上实现 `resolveRouteBinding()`。

```php
use App\Models\AiConversation;
use Illuminate\Support\Facades\Route;

Route::model('conversation', AiConversation::class);
```

### 服务容器绑定

在服务提供者中绑定接口到实现以进行清晰的依赖接线。

```php
use App\Repositories\EloquentOrderRepository;
use App\Repositories\OrderRepository;
use Illuminate\Support\ServiceProvider;

final class AppServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->app->bind(OrderRepository::class, EloquentOrderRepository::class);
    }
}
```

### Eloquent 模型模式

#### 模型配置

```php
final class Project extends Model
{
    use HasFactory;

    protected $fillable = ['name', 'owner_id', 'status'];

    protected $casts = [
        'status' => ProjectStatus::class,
        'archived_at' => 'datetime',
    ];

    public function owner(): BelongsTo
    {
        return $this->belongsTo(User::class, 'owner_id');
    }

    public function scopeActive(Builder $query): Builder
    {
        return $query->whereNull('archived_at');
    }
}
```

#### 自定义 Casts 和值对象

使用枚举或值对象进行严格类型。

```php
use Illuminate\Database\Eloquent\Casts\Attribute;

protected $casts = [
    'status' => ProjectStatus::class,
];
```

```php
protected function budgetCents(): Attribute
{
    return Attribute::make(
        get: fn (int $value) => Money::fromCents($value),
        set: fn (Money $money) => $money->toCents(),
    );
}
```

#### 预加载避免 N+1

```php
$orders = Order::query()
    ->with(['customer', 'items.product'])
    ->latest()
    ->paginate(25);
```

#### 查询对象用于复杂过滤器

```php
final class ProjectQuery
{
    public function __construct(private Builder $query) {}

    public function ownedBy(int $userId): self
    {
        $query = clone $this->query;

        return new self($query->where('owner_id', $userId));
    }

    public function active(): self
    {
        $query = clone $this->query;

        return new self($query->whereNull('archived_at'));
    }

    public function builder(): Builder
    {
        return $this->query;
    }
}
```

#### 全局作用域和软删除

对默认过滤使用全局作用域，`SoftDeletes` 用于可恢复记录。
仅在打算分层行为时才同时使用全局作用域和命名作用域。

```php
use Illuminate\Database\Eloquent\SoftDeletes;
use Illuminate\Database\Eloquent\Builder;

final class Project extends Model
{
    use SoftDeletes;

    protected static function booted(): void
    {
        static::addGlobalScope('active', function (Builder $builder): void {
            $builder->whereNull('archived_at');
        });
    }
}
```

#### 查询作用域用于可重用过滤器

```php
use Illuminate\Database\Eloquent\Builder;

final class Project extends Model
{
    public function scopeOwnedBy(Builder $query, int $userId): Builder
    {
        return $query->where('owner_id', $userId);
    }
}

// 在服务、仓库等中
$projects = Project::ownedBy($user->id)->get();
```

#### 多步更新的事务

```php
use Illuminate\Support\Facades\DB;

DB::transaction(function (): void {
    $order->update(['status' => 'paid']);
    $order->items()->update(['paid_at' => now()]);
});
```

### 迁移

#### 命名约定

- 文件名使用时间戳：`YYYY_MM_DD_HHMMSS_create_users_table.php`
- 迁移使用匿名类（无命名类）；文件名传达意图
- 表名为 `snake_case` 默认复数

#### 迁移示例

```php
use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('orders', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('customer_id')->constrained()->cascadeOnDelete();
            $table->string('status', 32)->index();
            $table->unsignedInteger('total_cents');
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('orders');
    }
};
```

### 表单请求和验证

将验证保持在表单请求中并将输入转换为 DTO。

```php
use App\Models\Order;

final class StoreOrderRequest extends FormRequest
{
    public function authorize(): bool
    {
        return $this->user()?->can('create', Order::class) ?? false;
    }

    public function rules(): array
    {
        return [
            'customer_id' => ['required', 'integer', 'exists:customers,id'],
            'items' => ['required', 'array', 'min:1'],
            'items.*.sku' => ['required', 'string'],
            'items.*.quantity' => ['required', 'integer', 'min:1'],
        ];
    }

    public function toDto(): CreateOrderData
    {
        return new CreateOrderData(
            customerId: (int) $this->validated('customer_id'),
            items: $this->validated('items'),
        );
    }
}
```

### API 资源

使用资源和分页保持 API 响应一致。

```php
$projects = Project::query()->active()->paginate(25);

return response()->json([
    'success' => true,
    'data' => ProjectResource::collection($projects->items()),
    'error' => null,
    'meta' => [
        'page' => $projects->currentPage(),
        'per_page' => $projects->perPage(),
        'total' => $projects->total(),
    ],
]);
```

### 事件、作业和队列

- 为副作用（电子邮件、分析）发出领域事件
- 使用队列作业进行慢速工作（报告、导出、webhooks）
- 优先使用带重试和退避的幂等处理程序

### 缓存

- 缓存读取重度的端点和昂贵查询
- 在模型事件（创建/更新/删除）时使缓存失效
- 缓存相关数据时使用标签以便轻松失效

### 配置和环境

- secrets 保持在 `.env` 中，配置在 `config/*.php` 中
- 使用按环境配置覆盖和在生产中 `config:cache`

---

---
name: laravel-security
description: Laravel 安全最佳实践，用于认证/授权、验证、CSRF、大量赋值、文件上传、secrets、速率限制和安全部署。
origin: ECC
---

# Laravel 安全最佳实践

保护 Laravel 应用免受常见漏洞的综合安全指南。

## 何时使用

- 添加认证或授权
- 处理用户输入和文件上传
- 构建新的 API 端点
- 管理 secrets 和环境设置
- 加固生产部署

## 工作原理

- 中间件提供基线保护（通过 `VerifyCsrfToken` 的 CSRF、通过 `SecurityHeaders` 的安全头）。
- 守卫和策略强制访问控制（`auth:sanctum`、`$this->authorize`、策略中间件）。
- 表单请求在到达服务之前验证和塑造输入（`UploadInvoiceRequest`）。
- 速率限制在认证控制之上添加滥用保护（`RateLimiter::for('login')`）。
- 数据安全来自加密 casts、大量赋值守卫和签名路由（`URL::temporarySignedRoute` + `signed` 中间件）。

## 核心安全设置

- `APP_DEBUG=false` 在生产中
- `APP_KEY` 必须设置并在泄露时轮换
- 设置 `SESSION_SECURE_COOKIE=true` 和 `SESSION_SAME_SITE=lax`（或对敏感应用使用 `strict`）
- 配置可信代理以正确检测 HTTPS

## 会话和 Cookie 加固

- 设置 `SESSION_HTTP_ONLY=true` 防止 JavaScript 访问
- 对高风险流程使用 `SESSION_SAME_SITE=strict`
- 在登录和权限更改时重新生成会话

## 认证和令牌

- 使用 Laravel Sanctum 或 Passport 进行 API 认证
- 对敏感数据优先使用短寿命令牌和刷新流
- 在注销和泄露账户时撤销令牌

示例路由保护：

```php
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;

Route::middleware('auth:sanctum')->get('/me', function (Request $request) {
    return $request->user();
});
```

## 密码安全

- 使用 `Hash::make()` 哈希密码绝不存储明文
- 使用 Laravel 的密码 broker 进行重置流

```php
use Illuminate\Support\Facades\Hash;
use Illuminate\Validation\Rules\Password;

$validated = $request->validate([
    'password' => ['required', 'string', Password::min(12)->letters()->mixedCase()->numbers()->symbols()],
]);

$user->update(['password' => Hash::make($validated['password'])]);
```

## 授权：策略和门

- 对模型级授权使用策略
- 在控制器和服务中强制授权

```php
$this->authorize('update', $project);
```

使用策略中间件进行路由级强制：

```php
use Illuminate\Support\Facades\Route;

Route::put('/projects/{project}', [ProjectController::class, 'update'])
    ->middleware(['auth:sanctum', 'can:update,project']);
```

## 验证和数据清理

- 始终使用表单请求验证输入
- 使用严格验证规则和类型检查
- 绝不信任请求有效载荷用于派生字段

## 大量赋值保护

- 使用 `$fillable` 或 `$guarded` 并避免 `Model::unguard()`
- 优先使用 DTO 或显式属性映射

## SQL 注入预防

- 使用 Eloquent 或查询构建器参数绑定
- 除非严格必要否则避免原始 SQL

```php
DB::select('select * from users where email = ?', [$email]);
```

## XSS 预防

- Blade 默认转义输出（`{{ }}`）
- 仅对可信、清理的 HTML 使用 `{!! !!}`
- 使用专用库清理富文本

## CSRF 保护

- 保持 `VerifyCsrfToken` 中间件启用
- 在表单中包含 `@csrf` 并为 SPA 请求发送 XSRF 令牌

对于使用 Sanctum 的 SPA 认证，确保配置有状态请求：

```php
// config/sanctum.php
'stateful' => explode(',', env('SANCTUM_STATEFUL_DOMAINS', 'localhost')),
```

## 文件上传安全

- 验证文件大小、MIME 类型和扩展名
- 尽可能将上传存储在公共路径之外
- 需要时扫描文件中的恶意软件

```php
final class UploadInvoiceRequest extends FormRequest
{
    public function authorize(): bool
    {
        return (bool) $this->user()?->can('upload-invoice');
    }

    public function rules(): array
    {
        return [
            'invoice' => ['required', 'file', 'mimes:pdf', 'max:5120'],
        ];
    }
}
```

```php
$path = $request->file('invoice')->store(
    'invoices',
    config('filesystems.private_disk', 'local') // 设置为非公共磁盘
);
```

## 速率限制

- 在认证和写端点上应用 `throttle` 中间件
- 对登录、密码重置和 OTP 使用更严格的限制

```php
use Illuminate\Cache\RateLimiting\Limit;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\RateLimiter;

RateLimiter::for('login', function (Request $request) {
    return [
        Limit::perMinute(5)->by($request->ip()),
        Limit::perMinute(5)->by(strtolower((string) $request->input('email'))),
    ];
});
```

## Secrets 和凭证

- 绝不提交 secrets 到源代码控制
- 使用环境变量和 secrets 管理器
- 泄露后轮换密钥并使会话失效

## 加密属性

对静态敏感列使用加密 casts。

```php
protected $casts = [
    'api_token' => 'encrypted',
];
```

## 安全头

- 在适当时添加 CSP、HSTS 和帧保护
- 使用可信代理配置强制 HTTPS 重定向

设置头的示例中间件：

```php
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

final class SecurityHeaders
{
    public function handle(Request $request, \Closure $next): Response
    {
        $response = $next($request);

        $response->headers->add([
            'Content-Security-Policy' => "default-src 'self'",
            'Strict-Transport-Security' => 'max-age=31536000', // 仅在所有子域都是 HTTPS 时添加 includeSubDomains/preload
            'X-Frame-Options' => 'DENY',
            'X-Content-Type-Options' => 'nosniff',
            'Referrer-Policy' => 'no-referrer',
        ]);

        return $response;
    }
}
```

## CORS 和 API 暴露

- 在 `config/cors.php` 中限制来源
- 对认证路由避免通配符来源

```php
// config/cors.php
return [
    'paths' => ['api/*', 'sanctum/csrf-cookie'],
    'allowed_methods' => ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'],
    'allowed_origins' => ['https://app.example.com'],
    'allowed_headers' => [
        'Content-Type',
        'Authorization',
        'X-Requested-With',
        'X-XSRF-TOKEN',
        'X-CSRF-TOKEN',
    ],
    'supports_credentials' => true,
];
```

## 日志和 PII

- 绝不记录密码、token 或完整卡数据
- 在结构化日志中删除敏感字段

```php
use Illuminate\Support\Facades\Log;

Log::info('User updated profile', [
    'user_id' => $user->id,
    'email' => '[REDACTED]',
    'token' => '[REDACTED]',
]);
```

## 依赖安全

- 定期运行 `composer audit`
- 谨慎固定依赖并在 CVE 时及时更新

## 签名 URL

对临时、防篡改链接使用签名路由。

```php
use Illuminate\Support\Facades\URL;

$url = URL::temporarySignedRoute(
    'downloads.invoice',
    now()->addMinutes(15),
    ['invoice' => $invoice->id]
);
```

```php
use Illuminate\Support\Facades\Route;

Route::get('/invoices/{invoice}/download', [InvoiceController::class, 'download'])
    ->name('downloads.invoice')
    ->middleware('signed');
```

---

---
name: laravel-tdd
description: 使用 PHPUnit 和 Pest 进行 Laravel 测试驱动开发，包括工厂、数据库测试、fakes 和覆盖目标。
origin: ECC
---

# Laravel TDD 工作流

使用 PHPUnit 和 Pest 进行 Laravel 应用测试驱动开发，目标覆盖 80%+（单元 + 功能）。

## 何时使用

- Laravel 中的新功能或端点
- Bug 修复或重构
- 测试 Eloquent 模型、策略、作业和通知
- 除非项目已标准化于 PHPUnit，否则优先使用 Pest

## 工作原理

### 红-绿-重构循环

1) 写一个失败的测试
2) 实现最小更改通过
3) 在保持测试绿色时重构

### 测试层级

- **单元**：纯 PHP 类、值对象、服务
- **功能**：HTTP 端点、认证、验证、策略
- **集成**：数据库 + 队列 + 外部边界

根据范围选择层级：

- 对纯业务逻辑和服务使用 **单元** 测试。
- 对 HTTP、认证、验证和响应形状使用 **功能** 测试。
- 当一起验证 DB/队列/外部服务时使用 **集成** 测试。

### 数据库策略

- `RefreshDatabase` 用于大多数功能/集成测试（支持事务时每个测试运行运行一次迁移，然后每个测试包装在事务中；`:memory:` SQLite 或无事务连接可能在每个测试前重新迁移）
- `DatabaseTransactions` 当 schema 已经迁移且只需要每个测试回滚时
- `DatabaseMigrations` 当需要每个测试完全迁移/新鲜且可以承受成本时

将 `RefreshDatabase` 作为触碰数据库的测试的默认选项：对于支持事务的数据库，它运行一次迁移每个测试运行（通过静态标志）并包装每个测试在事务中；对于 `:memory:` SQLite 或无事务连接，它在每个测试前迁移。使用 `DatabaseTransactions` 当 schema 已经迁移且你只需要每个测试回滚。

### 测试框架选择

- 有可用时默认使用 **Pest** 用于新测试。
- 仅在项目已标准化于 PHPUnit 或需要 PHPUnit 特定工具时才使用 **PHPUnit**。

## 示例

### PHPUnit 示例

```php
use App\Models\User;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Tests\TestCase;

final class ProjectControllerTest extends TestCase
{
    use RefreshDatabase;

    public function test_owner_can_create_project(): void
    {
        $user = User::factory()->create();

        $response = $this->actingAs($user)->postJson('/api/projects', [
            'name' => 'New Project',
        ]);

        $response->assertCreated();
        $this->assertDatabaseHas('projects', ['name' => 'New Project']);
    }
}
```

### 功能测试示例（HTTP 层）

```php
use App\Models\Project;
use App\Models\User;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Tests\TestCase;

final class ProjectIndexTest extends TestCase
{
    use RefreshDatabase;

    public function test_projects_index_returns_paginated_results(): void
    {
        $user = User::factory()->create();
        Project::factory()->count(3)->for($user)->create();

        $response = $this->actingAs($user)->getJson('/api/projects');

        $response->assertOk();
        $response->assertJsonStructure(['success', 'data', 'error', 'meta']);
    }
}
```

### Pest 示例

```php
use App\Models\User;
use Illuminate\Foundation\Testing\RefreshDatabase;

use function Pest\Laravel\actingAs;
use function Pest\Laravel\assertDatabaseHas;

uses(RefreshDatabase::class);

test('owner can create project', function () {
    $user = User::factory()->create();

    $response = actingAs($user)->postJson('/api/projects', [
        'name' => 'New Project',
    ]);

    $response->assertCreated();
    assertDatabaseHas('projects', ['name' => 'New Project']);
});
```

### 功能测试 Pest 示例（HTTP 层）

```php
use App\Models\Project;
use App\Models\User;
use Illuminate\Foundation\Testing\RefreshDatabase;

use function Pest\Laravel\actingAs;

uses(RefreshDatabase::class);

test('projects index returns paginated results', function () {
    $user = User::factory()->create();
    Project::factory()->count(3)->for($user)->create();

    $response = actingAs($user)->getJson('/api/projects');

    $response->assertOk();
    $response->assertJsonStructure(['success', 'data', 'error', 'meta']);
});
```

### 工厂和状态

- 使用工厂进行测试数据
- 为边界情况定义状态（archived、admin、trial）

```php
$user = User::factory()->state(['role' => 'admin'])->create();
```

### 数据库测试

- 使用 `RefreshDatabase` 保持干净状态
- 保持测试隔离和确定性
- 优先 `assertDatabaseHas` 而非手动查询

### 持久化测试示例

```php
use App\Models\Project;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Tests\TestCase;

final class ProjectRepositoryTest extends TestCase
{
    use RefreshDatabase;

    public function test_project_can_be_retrieved_by_slug(): void
    {
        $project = Project::factory()->create(['slug' => 'alpha']);

        $found = Project::query()->where('slug', 'alpha')->firstOrFail();

        $this->assertSame($project->id, $found->id);
    }
}
```

### Fakes 用于副作用

- `Bus::fake()` 用于作业
- `Queue::fake()` 用于队列工作
- `Mail::fake()` 和 `Notification::fake()` 用于通知
- `Event::fake()` 用于领域事件

```php
use Illuminate\Support\Facades\Queue;

Queue::fake();

dispatch(new SendOrderConfirmation($order->id));

Queue::assertPushed(SendOrderConfirmation::class);
```

```php
use Illuminate\Support\Facades\Notification;

Notification::fake();

$user->notify(new InvoiceReady($invoice));

Notification::assertSentTo($user, InvoiceReady::class);
```

### Auth 测试（Sanctum）

```php
use Laravel\Sanctum\Sanctum;

Sanctum::actingAs($user);

$response = $this->getJson('/api/projects');
$response->assertOk();
```

### HTTP 和外部服务

- 使用 `Http::fake()` 隔离外部 API
- 使用 `Http::assertSent()` 断言出站有效载荷

### 覆盖目标

- 对单元 + 功能测试强制 80%+ 覆盖
- 在 CI 中使用 `pcov` 或 `XDEBUG_MODE=coverage`

### 测试命令

- `php artisan test`
- `vendor/bin/phpunit`
- `vendor/bin/pest`

### 测试配置

- 使用 `phpunit.xml` 设置 `DB_CONNECTION=sqlite` 和 `DB_DATABASE=:memory:` 用于快速测试
- 保持单独的测试环境以避免触碰开发/生产数据

### 授权测试

```php
use Illuminate\Support\Facades\Gate;

$this->assertTrue(Gate::forUser($user)->allows('update', $project));
$this->assertFalse(Gate::forUser($otherUser)->allows('update', $project));
```

### Inertia 功能测试

使用 Inertia.js 时，使用 Inertia 测试助手断言组件名称和 props。

```php
use App\Models\User;
use Inertia\Testing\AssertableInertia;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Tests\TestCase;

final class DashboardInertiaTest extends TestCase
{
    use RefreshDatabase;

    public function test_dashboard_inertia_props(): void
    {
        $user = User::factory()->create();

        $response = $this->actingAs($user)->get('/dashboard');

        $response->assertOk();
        $response->assertInertia(fn (AssertableInertia $page) => $page
            ->component('Dashboard')
            ->where('user.id', $user->id)
            ->has('projects')
        );
    }
}
```

优先 `assertInertia` 而非原始 JSON 断言以保持测试与 Inertia 响应一致。

---

---
name: laravel-verification
description: Laravel 项目验证循环：环境检查、linting、静态分析、带覆盖的测试、安全扫描和部署就绪。
origin: ECC
---

# Laravel 验证循环

Laravel 项目的验证检查清单：环境检查、linting、静态分析、带覆盖的测试、安全扫描和部署就绪。

## 何时使用

- 准备部署到生产环境
- 验证代码质量和平稳性
- 在拉取请求中运行质量门
- 持续集成中的自动检查

## 验证阶段

### 阶段 1：本地环境检查

```bash
# 确认 .env 配置正确
php artisan config:cache
php artisan route:cache

# 验证没有敏感数据泄露
grep -r "APP_KEY" .env || echo "APP_KEY set"
grep -r "DEBUG=true" .env || echo "DEBUG=false"
```

### 阶段 2：代码质量

```bash
# PHPStan 静态分析
./vendor/bin/phpstan analyse app --memory-limit=512M

# Pint 代码风格
./vendor/bin/pint --test

# PHP CS Fixer 检查（如使用）
./vendor/bin/php-cs-fixer fix --dry-run --diff
```

### 阶段 3：测试

```bash
# 运行完整测试套件
php artisan test --coverage

# 或使用 Pest
./vendor/bin/pest --coverage

# 验证覆盖门槛
php artisan test --coverage-min=80
```

### 阶段 4：安全扫描

```bash
# 依赖审核
composer audit

# Laravel 安全最佳实践检查
# - APP_DEBUG=false
# - CSRF 启用
# - 速率限制配置
# - 加密 cast 在敏感字段上
```

### 阶段 5：部署检查

- [ ] `.env` 配置正确（生产值）
- [ ] `APP_KEY` 已设置
- [ ] `APP_DEBUG=false`
- [ ] 数据库迁移已运行
- [ ] 队列 worker 在运行
- [ ] 调度器已配置
- [ ] 日志正确路由
- [ ] 监控已设置