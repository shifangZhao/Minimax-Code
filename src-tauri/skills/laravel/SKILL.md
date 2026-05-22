---
name: laravel-patterns
description: Laravel architecture patterns, routing/controllers, Eloquent ORM, service layers, queues, events, caching, and API resources for production apps.
origin: ECC
---

# Laravel Development Patterns

Production-grade Laravel architecture patterns for scalable, maintainable applications.

## When to Use

- Building Laravel web applications or APIs
- Structuring controllers, services, and domain logic
- Working with Eloquent models and relationships
- Designing APIs with resources and pagination
- Adding queues, events, caching, and background jobs

## How It Works

- Structure the app around clear boundaries (controllers -> services/actions -> models).
- Use explicit bindings and scoped bindings to keep routing predictable; still enforce authorization for access control.
- Favor typed models, casts, and scopes to keep domain logic consistent.
- Keep IO-heavy work in queues and cache expensive reads.
- Centralize config in `config/*` and keep environments explicit.

## Examples

### Project Structure

Use a conventional Laravel layout with clear layer boundaries (HTTP, services/actions, models).

### Recommended Layout

```
app/
├── Actions/            # Single-purpose use cases
├── Console/
├── Events/
├── Exceptions/
├── Http/
│   ├── Controllers/
│   ├── Middleware/
│   ├── Requests/       # Form request validation
│   └── Resources/      # API resources
├── Jobs/
├── Models/
├── Policies/
├── Providers/
├── Services/           # Coordinating domain services
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

### Controllers -> Services -> Actions

Keep controllers thin. Put orchestration in services and single-purpose logic in actions.

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

### Routing and Controllers

Prefer route-model binding and resource controllers for clarity.

```php
use Illuminate\Support\Facades\Route;

Route::middleware('auth:sanctum')->group(function () {
    Route::apiResource('projects', ProjectController::class);
});
```

### Route Model Binding (Scoped)

Use scoped bindings to prevent cross-tenant access.

```php
Route::scopeBindings()->group(function () {
    Route::get('/accounts/{account}/projects/{project}', [ProjectController::class, 'show']);
});
```

### Nested Routes and Binding Names

- Keep prefixes and paths consistent to avoid double nesting (e.g., `conversation` vs `conversations`).
- Use a single parameter name that matches the bound model (e.g., `{conversation}` for `Conversation`).
- Prefer scoped bindings when nesting to enforce parent-child relationships.

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

If you want a parameter to resolve to a different model class, define explicit binding. For custom binding logic, use `Route::bind()` or implement `resolveRouteBinding()` on the model.

```php
use App\Models\AiConversation;
use Illuminate\Support\Facades\Route;

Route::model('conversation', AiConversation::class);
```

### Service Container Bindings

Bind interfaces to implementations in a service provider for clear dependency wiring.

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

### Eloquent Model Patterns

### Model Configuration

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

### Custom Casts and Value Objects

Use enums or value objects for strict typing.

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

### Eager Loading to Avoid N+1

```php
$orders = Order::query()
    ->with(['customer', 'items.product'])
    ->latest()
    ->paginate(25);
```

### Query Objects for Complex Filters

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

### Global Scopes and Soft Deletes

Use global scopes for default filtering and `SoftDeletes` for recoverable records.
Use either a global scope or a named scope for the same filter, not both, unless you intend layered behavior.

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

### Query Scopes for Reusable Filters

```php
use Illuminate\Database\Eloquent\Builder;

final class Project extends Model
{
    public function scopeOwnedBy(Builder $query, int $userId): Builder
    {
        return $query->where('owner_id', $userId);
    }
}

// In service, repository etc.
$projects = Project::ownedBy($user->id)->get();
```

### Transactions for Multi-Step Updates

```php
use Illuminate\Support\Facades\DB;

DB::transaction(function (): void {
    $order->update(['status' => 'paid']);
    $order->items()->update(['paid_at' => now()]);
});
```

### Migrations

### Naming Convention

- File names use timestamps: `YYYY_MM_DD_HHMMSS_create_users_table.php`
- Migrations use anonymous classes (no named class); the filename communicates intent
- Table names are `snake_case` and plural by default

### Example Migration

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

### Form Requests and Validation

Keep validation in form requests and transform inputs to DTOs.

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

### API Resources

Keep API responses consistent with resources and pagination.

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

### Events, Jobs, and Queues

- Emit domain events for side effects (emails, analytics)
- Use queued jobs for slow work (reports, exports, webhooks)
- Prefer idempotent handlers with retries and backoff

### Caching

- Cache read-heavy endpoints and expensive queries
- Invalidate caches on model events (created/updated/deleted)
- Use tags when caching related data for easy invalidation

### Configuration and Environments

- Keep secrets in `.env` and config in `config/*.php`
- Use per-environment config overrides and `config:cache` in production


---

---
name: laravel-security
description: Laravel security best practices for authn/authz, validation, CSRF, mass assignment, file uploads, secrets, rate limiting, and secure deployment.
origin: ECC
---

# Laravel Security Best Practices

Comprehensive security guidance for Laravel applications to protect against common vulnerabilities.

## When to Activate

- Adding authentication or authorization
- Handling user input and file uploads
- Building new API endpoints
- Managing secrets and environment settings
- Hardening production deployments

## How It Works

- Middleware provides baseline protections (CSRF via `VerifyCsrfToken`, security headers via `SecurityHeaders`).
- Guards and policies enforce access control (`auth:sanctum`, `$this->authorize`, policy middleware).
- Form Requests validate and shape input (`UploadInvoiceRequest`) before it reaches services.
- Rate limiting adds abuse protection (`RateLimiter::for('login')`) alongside auth controls.
- Data safety comes from encrypted casts, mass-assignment guards, and signed routes (`URL::temporarySignedRoute` + `signed` middleware).

## Core Security Settings

- `APP_DEBUG=false` in production
- `APP_KEY` must be set and rotated on compromise
- Set `SESSION_SECURE_COOKIE=true` and `SESSION_SAME_SITE=lax` (or `strict` for sensitive apps)
- Configure trusted proxies for correct HTTPS detection

## Session and Cookie Hardening

- Set `SESSION_HTTP_ONLY=true` to prevent JavaScript access
- Use `SESSION_SAME_SITE=strict` for high-risk flows
- Regenerate sessions on login and privilege changes

## Authentication and Tokens

- Use Laravel Sanctum or Passport for API auth
- Prefer short-lived tokens with refresh flows for sensitive data
- Revoke tokens on logout and compromised accounts

Example route protection:

```php
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;

Route::middleware('auth:sanctum')->get('/me', function (Request $request) {
    return $request->user();
});
```

## Password Security

- Hash passwords with `Hash::make()` and never store plaintext
- Use Laravel's password broker for reset flows

```php
use Illuminate\Support\Facades\Hash;
use Illuminate\Validation\Rules\Password;

$validated = $request->validate([
    'password' => ['required', 'string', Password::min(12)->letters()->mixedCase()->numbers()->symbols()],
]);

$user->update(['password' => Hash::make($validated['password'])]);
```

## Authorization: Policies and Gates

- Use policies for model-level authorization
- Enforce authorization in controllers and services

```php
$this->authorize('update', $project);
```

Use policy middleware for route-level enforcement:

```php
use Illuminate\Support\Facades\Route;

Route::put('/projects/{project}', [ProjectController::class, 'update'])
    ->middleware(['auth:sanctum', 'can:update,project']);
```

## Validation and Data Sanitization

- Always validate inputs with Form Requests
- Use strict validation rules and type checks
- Never trust request payloads for derived fields

## Mass Assignment Protection

- Use `$fillable` or `$guarded` and avoid `Model::unguard()`
- Prefer DTOs or explicit attribute mapping

## SQL Injection Prevention

- Use Eloquent or query builder parameter binding
- Avoid raw SQL unless strictly necessary

```php
DB::select('select * from users where email = ?', [$email]);
```

## XSS Prevention

- Blade escapes output by default (`{{ }}`)
- Use `{!! !!}` only for trusted, sanitized HTML
- Sanitize rich text with a dedicated library

## CSRF Protection

- Keep `VerifyCsrfToken` middleware enabled
- Include `@csrf` in forms and send XSRF tokens for SPA requests

For SPA authentication with Sanctum, ensure stateful requests are configured:

```php
// config/sanctum.php
'stateful' => explode(',', env('SANCTUM_STATEFUL_DOMAINS', 'localhost')),
```

## File Upload Safety

- Validate file size, MIME type, and extension
- Store uploads outside the public path when possible
- Scan files for malware if required

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
    config('filesystems.private_disk', 'local') // set this to a non-public disk
);
```

## Rate Limiting

- Apply `throttle` middleware on auth and write endpoints
- Use stricter limits for login, password reset, and OTP

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

## Secrets and Credentials

- Never commit secrets to source control
- Use environment variables and secret managers
- Rotate keys after exposure and invalidate sessions

## Encrypted Attributes

Use encrypted casts for sensitive columns at rest.

```php
protected $casts = [
    'api_token' => 'encrypted',
];
```

## Security Headers

- Add CSP, HSTS, and frame protection where appropriate
- Use trusted proxy configuration to enforce HTTPS redirects

Example middleware to set headers:

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
            'Strict-Transport-Security' => 'max-age=31536000', // add includeSubDomains/preload only when all subdomains are HTTPS
            'X-Frame-Options' => 'DENY',
            'X-Content-Type-Options' => 'nosniff',
            'Referrer-Policy' => 'no-referrer',
        ]);

        return $response;
    }
}
```

## CORS and API Exposure

- Restrict origins in `config/cors.php`
- Avoid wildcard origins for authenticated routes

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

## Logging and PII

- Never log passwords, tokens, or full card data
- Redact sensitive fields in structured logs

```php
use Illuminate\Support\Facades\Log;

Log::info('User updated profile', [
    'user_id' => $user->id,
    'email' => '[REDACTED]',
    'token' => '[REDACTED]',
]);
```

## Dependency Security

- Run `composer audit` regularly
- Pin dependencies with care and update promptly on CVEs

## Signed URLs

Use signed routes for temporary, tamper-proof links.

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
description: Test-driven development for Laravel with PHPUnit and Pest, factories, database testing, fakes, and coverage targets.
origin: ECC
---

# Laravel TDD Workflow

Test-driven development for Laravel applications using PHPUnit and Pest with 80%+ coverage (unit + feature).

## When to Use

- New features or endpoints in Laravel
- Bug fixes or refactors
- Testing Eloquent models, policies, jobs, and notifications
- Prefer Pest for new tests unless the project already standardizes on PHPUnit

## How It Works

### Red-Green-Refactor Cycle

1) Write a failing test
2) Implement the minimal change to pass
3) Refactor while keeping tests green

### Test Layers

- **Unit**: pure PHP classes, value objects, services
- **Feature**: HTTP endpoints, auth, validation, policies
- **Integration**: database + queue + external boundaries

Choose layers based on scope:

- Use **Unit** tests for pure business logic and services.
- Use **Feature** tests for HTTP, auth, validation, and response shape.
- Use **Integration** tests when validating DB/queues/external services together.

### Database Strategy

- `RefreshDatabase` for most feature/integration tests (runs migrations once per test run, then wraps each test in a transaction when supported; in-memory databases may re-migrate per test)
- `DatabaseTransactions` when the schema is already migrated and you only need per-test rollback
- `DatabaseMigrations` when you need a full migrate/fresh for every test and can afford the cost

Use `RefreshDatabase` as the default for tests that touch the database: for databases with transaction support, it runs migrations once per test run (via a static flag) and wraps each test in a transaction; for `:memory:` SQLite or connections without transactions, it migrates before each test. Use `DatabaseTransactions` when the schema is already migrated and you only need per-test rollbacks.

### Testing Framework Choice

- Default to **Pest** for new tests when available.
- Use **PHPUnit** only if the project already standardizes on it or requires PHPUnit-specific tooling.

## Examples

### PHPUnit Example

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

### Feature Test Example (HTTP Layer)

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

### Pest Example

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

### Feature Test Pest Example (HTTP Layer)

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

### Factories and States

- Use factories for test data
- Define states for edge cases (archived, admin, trial)

```php
$user = User::factory()->state(['role' => 'admin'])->create();
```

### Database Testing

- Use `RefreshDatabase` for clean state
- Keep tests isolated and deterministic
- Prefer `assertDatabaseHas` over manual queries

### Persistence Test Example

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

### Fakes for Side Effects

- `Bus::fake()` for jobs
- `Queue::fake()` for queued work
- `Mail::fake()` and `Notification::fake()` for notifications
- `Event::fake()` for domain events

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

### Auth Testing (Sanctum)

```php
use Laravel\Sanctum\Sanctum;

Sanctum::actingAs($user);

$response = $this->getJson('/api/projects');
$response->assertOk();
```

### HTTP and External Services

- Use `Http::fake()` to isolate external APIs
- Assert outbound payloads with `Http::assertSent()`

### Coverage Targets

- Enforce 80%+ coverage for unit + feature tests
- Use `pcov` or `XDEBUG_MODE=coverage` in CI

### Test Commands

- `php artisan test`
- `vendor/bin/phpunit`
- `vendor/bin/pest`

### Test Configuration

- Use `phpunit.xml` to set `DB_CONNECTION=sqlite` and `DB_DATABASE=:memory:` for fast tests
- Keep separate env for tests to avoid touching dev/prod data

### Authorization Tests

```php
use Illuminate\Support\Facades\Gate;

$this->assertTrue(Gate::forUser($user)->allows('update', $project));
$this->assertFalse(Gate::forUser($otherUser)->allows('update', $project));
```

### Inertia Feature Tests

When using Inertia.js, assert on the component name and props with the Inertia testing helpers.

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

Prefer `assertInertia` over raw JSON assertions to keep tests aligned with Inertia responses.


---

---
name: laravel-verification
description: "Verification loop for Laravel projects: env checks, linting, static analysis, tests with coverage, security scans, and deployment readiness."
origin: ECC
---

# Laravel Verification Loop

Run before PRs, after major changes, and pre-deploy.

## When to Use

- Before opening a pull request for a Laravel project
- After major refactors or dependency upgrades
- Pre-deployment verification for staging or production
- Running full lint -> test -> security -> deploy readiness pipeline

## How It Works

- Run phases sequentially from environment checks through deployment readiness so each layer builds on the last.
- Environment and Composer checks gate everything else; stop immediately if they fail.
- Linting/static analysis should be clean before running full tests and coverage.
- Security and migration reviews happen after tests so you verify behavior before data or release steps.
- Build/deploy readiness and queue/scheduler checks are final gates; any failure blocks release.

## Phase 1: Environment Checks

```bash
php -v
composer --version
php artisan --version
```

- Verify `.env` is present and required keys exist
- Confirm `APP_DEBUG=false` for production environments
- Confirm `APP_ENV` matches the target deployment (`production`, `staging`)

If using Laravel Sail locally:

```bash
./vendor/bin/sail php -v
./vendor/bin/sail artisan --version
```

## Phase 1.5: Composer and Autoload

```bash
composer validate
composer dump-autoload -o
```

## Phase 2: Linting and Static Analysis

```bash
vendor/bin/pint --test
vendor/bin/phpstan analyse
```

If your project uses Psalm instead of PHPStan:

```bash
vendor/bin/psalm
```

## Phase 3: Tests and Coverage

```bash
php artisan test
```

Coverage (CI):

```bash
XDEBUG_MODE=coverage php artisan test --coverage
```

CI example (format -> static analysis -> tests):

```bash
vendor/bin/pint --test
vendor/bin/phpstan analyse
XDEBUG_MODE=coverage php artisan test --coverage
```

## Phase 4: Security and Dependency Checks

```bash
composer audit
```

## Phase 5: Database and Migrations

```bash
php artisan migrate --pretend
php artisan migrate:status
```

- Review destructive migrations carefully
- Ensure migration filenames follow `Y_m_d_His_*` (e.g., `2025_03_14_154210_create_orders_table.php`) and describe the change clearly
- Ensure rollbacks are possible
- Verify `down()` methods and avoid irreversible data loss without explicit backups

## Phase 6: Build and Deployment Readiness

```bash
php artisan optimize:clear
php artisan config:cache
php artisan route:cache
php artisan view:cache
```

- Ensure cache warmups succeed in production configuration
- Verify queue workers and scheduler are configured
- Confirm `storage/` and `bootstrap/cache/` are writable in the target environment

## Phase 7: Queue and Scheduler Checks

```bash
php artisan schedule:list
php artisan queue:failed
```

If Horizon is used:

```bash
php artisan horizon:status
```

If `queue:monitor` is available, use it to check backlog without processing jobs:

```bash
php artisan queue:monitor default --max=100
```

Active verification (staging only): dispatch a no-op job to a dedicated queue and run a single worker to process it (ensure a non-`sync` queue connection is configured).

```bash
php artisan tinker --execute="dispatch((new App\\Jobs\\QueueHealthcheck())->onQueue('healthcheck'))"
php artisan queue:work --once --queue=healthcheck
```

Verify the job produced the expected side effect (log entry, healthcheck table row, or metric).

Only run this on non-production environments where processing a test job is safe.

## Examples

Minimal flow:

```bash
php -v
composer --version
php artisan --version
composer validate
vendor/bin/pint --test
vendor/bin/phpstan analyse
php artisan test
composer audit
php artisan migrate --pretend
php artisan config:cache
php artisan queue:failed
```

CI-style pipeline:

```bash
composer validate
composer dump-autoload -o
vendor/bin/pint --test
vendor/bin/phpstan analyse
XDEBUG_MODE=coverage php artisan test --coverage
composer audit
php artisan migrate --pretend
php artisan optimize:clear
php artisan config:cache
php artisan route:cache
php artisan view:cache
php artisan schedule:list
```


---

---
name: laravel-plugin-discovery
description: Discover and evaluate Laravel packages via LaraPlugins.io MCP. Use when the user wants to find plugins, check package health, or assess Laravel/PHP compatibility.
origin: ECC
---

# Laravel Plugin Discovery

Find, evaluate, and choose healthy Laravel packages using the LaraPlugins.io MCP server.

## When to Use

- User wants to find Laravel packages for a specific feature (e.g. "auth", "permissions", "admin panel")
- User asks "what package should I use for..." or "is there a Laravel package for..."
- User wants to check if a package is actively maintained
- User needs to verify Laravel version compatibility
- User wants to assess package health before adding to a project

## MCP Requirement

LaraPlugins MCP server must be configured. Add to your `~/.claude.json` mcpServers:

```json
"laraplugins": {
  "type": "http",
  "url": "https://laraplugins.io/mcp/plugins"
}
```

No API key required — the server is free for the Laravel community.

## MCP Tools

The LaraPlugins MCP provides two primary tools:

### SearchPluginTool

Search packages by keyword, health score, vendor, and version compatibility.

**Parameters:**
- `text_search` (string, optional): Keyword to search (e.g. "permission", "admin", "api")
- `health_score` (string, optional): Filter by health band — `Healthy`, `Medium`, `Unhealthy`, or `Unrated`
- `laravel_compatibility` (string, optional): Filter by Laravel version — `"5"`, `"6"`, `"7"`, `"8"`, `"9"`, `"10"`, `"11"`, `"12"`, `"13"`
- `php_compatibility` (string, optional): Filter by PHP version — `"7.4"`, `"8.0"`, `"8.1"`, `"8.2"`, `"8.3"`, `"8.4"`, `"8.5"`
- `vendor_filter` (string, optional): Filter by vendor name (e.g. "spatie", "laravel")
- `page` (number, optional): Page number for pagination

### GetPluginDetailsTool

Fetch detailed metrics, readme content, and version history for a specific package.

**Parameters:**
- `package` (string, required): Full Composer package name (e.g. "spatie/laravel-permission")
- `include_versions` (boolean, optional): Include version history in response

---

## How It Works

### Finding Packages

When the user wants to discover packages for a feature:

1. Use `SearchPluginTool` with relevant keywords
2. Apply filters for health score, Laravel version, or PHP version
3. Review the results with package names, descriptions, and health indicators

### Evaluating Packages

When the user wants to assess a specific package:

1. Use `GetPluginDetailsTool` with the package name
2. Review health score, last updated date, Laravel version support
3. Check vendor reputation and risk indicators

### Checking Compatibility

When the user needs Laravel or PHP version compatibility:

1. Search with `laravel_compatibility` filter set to their version
2. Or get details on a specific package to see its supported versions

---

## Examples

### Example: Find Authentication Packages

```
SearchPluginTool({
  text_search: "authentication",
  health_score: "Healthy"
})
```

Returns packages matching "authentication" with healthy status:
- spatie/laravel-permission
- laravel/breeze
- laravel/passport
- etc.

### Example: Find Laravel 12 Compatible Packages

```
SearchPluginTool({
  text_search: "admin panel",
  laravel_compatibility: "12"
})
```

Returns packages compatible with Laravel 12.

### Example: Get Package Details

```
GetPluginDetailsTool({
  package: "spatie/laravel-permission",
  include_versions: true
})
```

Returns:
- Health score and last activity
- Laravel/PHP version support
- Vendor reputation (risk score)
- Version history
- Brief description

### Example: Find Packages by Vendor

```
SearchPluginTool({
  vendor_filter: "spatie",
  health_score: "Healthy"
})
```

Returns all healthy packages from vendor "spatie".

---

## Filtering Best Practices

### By Health Score

| Health Band | Meaning |
|-------------|---------|
| `Healthy` | Active maintenance, recent updates |
| `Medium` | Occasional updates, may need attention |
| `Unhealthy` | Abandoned or infrequently maintained |
| `Unrated` | Not yet assessed |

**Recommendation**: Prefer `Healthy` packages for production applications.

### By Laravel Version

| Version | Notes |
|---------|-------|
| `13` | Latest Laravel |
| `12` | Current stable |
| `11` | Still widely used |
| `10` | Legacy but common |
| `5`-`9` | Deprecated |

**Recommendation**: Match the target project's Laravel version.

### Combining Filters

```typescript
// Find healthy, Laravel 12 compatible packages for permissions
SearchPluginTool({
  text_search: "permission",
  health_score: "Healthy",
  laravel_compatibility: "12"
})
```

---

## Response Interpretation

### Search Results

Each result includes:
- Package name (e.g. `spatie/laravel-permission`)
- Brief description
- Health status indicator
- Laravel version support badges

### Package Details

The detailed response includes:
- **Health Score**: Numeric or band indicator
- **Last Activity**: When the package was last updated
- **Laravel Support**: Version compatibility matrix
- **PHP Support**: PHP version compatibility
- **Risk Score**: Vendor trust indicators
- **Version History**: Recent release timeline

---

## Common Use Cases

| Scenario | Recommended Approach |
|----------|---------------------|
| "What package for auth?" | Search "auth" with healthy filter |
| "Is spatie/package still maintained?" | Get details, check health score |
| "Need Laravel 12 packages" | Search with laravel_compatibility: "12" |
| "Find admin panel packages" | Search "admin panel", review results |
| "Check vendor reputation" | Search by vendor, check details |

---

## Best Practices

1. **Always filter by health** — Use `health_score: "Healthy"` for production projects
2. **Match Laravel version** — Always check `laravel_compatibility` matches the target project
3. **Check vendor reputation** — Prefer packages from known vendors (spatie, laravel, etc.)
4. **Review before recommending** — Use GetPluginDetailsTool for a comprehensive assessment
5. **No API key needed** — The MCP is free, no authentication required

---

## Related Skills

- `laravel-patterns` — Laravel architecture and patterns
- `laravel-tdd` — Test-driven development for Laravel
- `laravel-security` — Laravel security best practices
- `documentation-lookup` — General library documentation lookup (Context7)
