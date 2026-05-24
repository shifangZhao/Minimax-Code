---
name: kotlin-patterns
description: 用于构建健壮、高效和可维护 Kotlin 应用程序的惯用 Kotlin 模式、最佳实践和约定，包括协程、空安全、DSL 构建器和不可变数据类。
origin: ECC
---

# Kotlin 开发模式

用于构建健壮、高效和可维护应用程序的惯用 Kotlin 模式和最佳实践。

## 何时使用

- 编写新的 Kotlin 代码
- 审查 Kotlin 代码
- 重构现有 Kotlin 代码
- 设计 Kotlin 模块或库
- 配置 Gradle Kotlin DSL 构建

## 工作原理

此技能在七个关键领域强制执行惯用 Kotlin 约定：使用类型系统和安全调用操作符的空安全、使用 `val` 和数据类的 `copy()` 的不可变性、用于穷举类型层次结构的密封类和使用协程和 `Flow` 的结构化并发、用于在不继承的情况下添加行为的扩展函数、使用 `@DslMarker` 和 lambda 接收器的类型安全 DSL 构建器，以及用于构建配置的 Gradle Kotlin DSL。

## 示例

**空安全与 Elvis 操作符：**
```kotlin
fun getUserEmail(userId: String): String {
    val user = userRepository.findById(userId)
    return user?.email ?: "unknown@example.com"
}
```

**用于穷举结果的密封类：**
```kotlin
sealed class Result<out T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Failure(val error: AppError) : Result<Nothing>()
    data object Loading : Result<Nothing>()
}
```

**使用 async/await 的结构化并发：**
```kotlin
suspend fun fetchUserWithPosts(userId: String): UserProfile =
    coroutineScope {
        val user = async { userService.getUser(userId) }
        val posts = async { postService.getUserPosts(userId) }
        UserProfile(user = user.await(), posts = posts.await())
    }
```

## 核心原则

### 1. 空安全

Kotlin 的类型系统区分可空和不可空类型。充分利用它。

```kotlin
// 好：默认使用不可空类型
fun getUser(id: String): User {
    return userRepository.findById(id)
        ?: throw UserNotFoundException("User $id not found")
}

// 好：安全调用和 Elvis 操作符
fun getUserEmail(userId: String): String {
    val user = userRepository.findById(userId)
    return user?.email ?: "unknown@example.com"
}

// 坏：强制解包可空类型
fun getUserEmail(userId: String): String {
    val user = userRepository.findById(userId)
    return user!!.email // 如果为 null 则抛出 NPE
}
```

### 2. 默认不可变

优先 `val` 而非 `var`，不可变集合而非可变集合。

```kotlin
// 好：不可变数据
data class User(
    val id: String,
    val name: String,
    val email: String,
)

// 好：使用 copy() 转换
fun updateEmail(user: User, newEmail: String): User =
    user.copy(email = newEmail)

// 好：不可变集合
val users: List<User> = listOf(user1, user2)
val filtered = users.filter { it.email.isNotBlank() }

// 坏：可变状态
var currentUser: User? = null // 避免可变全局状态
val mutableUsers = mutableListOf<User>() // 除非真正需要否则避免
```

### 3. 表达式体和单表达式函数

对简洁、可读的函数使用表达式体。

```kotlin
// 好：表达式体
fun isAdult(age: Int): Boolean = age >= 18

fun formatFullName(first: String, last: String): String =
    "$first $last".trim()

fun User.displayName(): String =
    name.ifBlank { email.substringBefore('@') }

// 好：when 作为表达式
fun statusMessage(code: Int): String = when (code) {
    200 -> "OK"
    404 -> "Not Found"
    500 -> "Internal Server Error"
    else -> "Unknown status: $code"
}

// 坏：不必要的块体
fun isAdult(age: Int): Boolean {
    return age >= 18
}
```

### 4. 数据类用于值对象

对主要持有数据的类型使用数据类。

```kotlin
// 好：带 copy、equals、hashCode、toString 的数据类
data class CreateUserRequest(
    val name: String,
    val email: String,
    val role: Role = Role.USER,
)

// 好：用于类型安全包装器的值类（运行时零开销）
@JvmInline
value class UserId(val value: String) {
    init {
        require(value.isNotBlank()) { "UserId cannot be blank" }
    }
}

@JvmInline
value class Email(val value: String) {
    init {
        require('@' in value) { "Invalid email: $value" }
    }
}

fun getUser(id: UserId): User = userRepository.findById(id)
```

## 密封类和接口

### 建模受限层次结构

```kotlin
// 好：用于穷举 when 的密封类
sealed class Result<out T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Failure(val error: AppError) : Result<Nothing>()
    data object Loading : Result<Nothing>()
}

fun <T> Result<T>.getOrNull(): T? = when (this) {
    is Result.Success -> data
    is Result.Failure -> null
    is Result.Loading -> null
}

fun <T> Result<T>.getOrThrow(): T = when (this) {
    is Result.Success -> data
    is Result.Failure -> throw error.toException()
    is Result.Loading -> throw IllegalStateException("Still loading")
}
```

### 用于 API 响应的密封接口

```kotlin
sealed interface ApiError {
    val message: String

    data class NotFound(override val message: String) : ApiError
    data class Unauthorized(override val message: String) : ApiError
    data class Validation(
        override val message: String,
        val field: String,
    ) : ApiError
    data class Internal(
        override val message: String,
        val cause: Throwable? = null,
    ) : ApiError
}

fun ApiError.toStatusCode(): Int = when (this) {
    is ApiError.NotFound -> 404
    is ApiError.Unauthorized -> 401
    is ApiError.Validation -> 422
    is ApiError.Internal -> 500
}
```

## 作用域函数

### 何时使用每个

```kotlin
// let：转换可空或作用域结果
val length: Int? = name?.let { it.trim().length }

// apply：配置对象（返回对象本身）
val user = User().apply {
    name = "Alice"
    email = "alice@example.com"
}

// also：副作用（返回对象本身）
val user = createUser(request).also { logger.info("Created user: ${it.id}") }

// run：使用接收器执行块（返回结果）
val result = connection.run {
    prepareStatement(sql)
    executeQuery()
}

// with：非扩展形式的 run
val csv = with(StringBuilder()) {
    appendLine("name,email")
    users.forEach { appendLine("${it.name},${it.email}") }
    toString()
}
```

### 反模式

```kotlin
// 坏：嵌套作用域函数
user?.let { u ->
    u.address?.let { addr ->
        addr.city?.let { city ->
            println(city) // 难读
        }
    }
}

// 好：链接安全调用
val city = user?.address?.city
city?.let { println(it) }
```

## 扩展函数

### 在不继承的情况下添加功能

```kotlin
// 好：特定领域的扩展
fun String.toSlug(): String =
    lowercase()
        .replace(Regex("[^a-z0-9\\s-]"), "")
        .replace(Regex("\\s+"), "-")
        .trim('-')

fun Instant.toLocalDate(zone: ZoneId = ZoneId.systemDefault()): LocalDate =
    atZone(zone).toLocalDate()

// 好：集合扩展
fun <T> List<T>.second(): T = this[1]

fun <T> List<T>.secondOrNull(): T? = getOrNull(1)

// 好：作用域扩展（不污染全局命名空间）
class UserService {
    private fun User.isActive(): Boolean =
        status == Status.ACTIVE && lastLogin.isAfter(Instant.now().minus(30, ChronoUnit.DAYS))

    fun getActiveUsers(): List<User> = userRepository.findAll().filter { it.isActive() }
}
```

## 协程

### 结构化并发

```kotlin
// 好：使用 coroutineScope 的结构化并发
suspend fun fetchUserWithPosts(userId: String): UserProfile =
    coroutineScope {
        val userDeferred = async { userService.getUser(userId) }
        val postsDeferred = async { postService.getUserPosts(userId) }

        UserProfile(
            user = userDeferred.await(),
            posts = postsDeferred.await(),
        )
    }

// 好：当子项可以独立失败时使用 supervisorScope
suspend fun fetchDashboard(userId: String): Dashboard =
    supervisorScope {
        val user = async { userService.getUser(userId) }
        val notifications = async { notificationService.getRecent(userId) }
        val recommendations = async { recommendationService.getFor(userId) }

        Dashboard(
            user = user.await(),
            notifications = try {
                notifications.await()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                emptyList()
            },
            recommendations = try {
                recommendations.await()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                emptyList()
            },
        )
    }
```

### Flow 用于响应式流

```kotlin
// 好：带适当错误处理的冷流
fun observeUsers(): Flow<List<User>> = flow {
    while (currentCoroutineContext().isActive) {
        val users = userRepository.findAll()
        emit(users)
        delay(5.seconds)
    }
}.catch { e ->
    logger.error("Error observing users", e)
    emit(emptyList())
}

// 好：Flow 操作符
fun searchUsers(query: Flow<String>): Flow<List<User>> =
    query
        .debounce(300.milliseconds)
        .distinctUntilChanged()
        .filter { it.length >= 2 }
        .mapLatest { q -> userRepository.search(q) }
        .catch { emit(emptyList()) }
```

### 取消和清理

```kotlin
// 好：尊重取消
suspend fun processItems(items: List<Item>) {
    items.forEach { item ->
        ensureActive() // 在昂贵工作前检查取消
        processItem(item)
    }
}

// 好：使用 try/finally 清理
suspend fun acquireAndProcess() {
    val resource = acquireResource()
    try {
        resource.process()
    } finally {
        withContext(NonCancellable) {
            resource.release() // 即使取消也始终释放
        }
    }
}
```

## 委托

### 属性委托

```kotlin
// 惰性初始化
val expensiveData: List<User> by lazy {
    userRepository.findAll()
}

// 可观察属性
var name: String by Delegates.observable("initial") { _, old, new ->
    logger.info("Name changed from '$old' to '$new'")
}

// Map 支持的属性
class Config(private val map: Map<String, Any?>) {
    val host: String by map
    val port: Int by map
    val debug: Boolean by map
}

val config = Config(mapOf("host" to "localhost", "port" to 8080, "debug" to true))
```

### 接口委托

```kotlin
// 好：委托接口实现
class LoggingUserRepository(
    private val delegate: UserRepository,
    private val logger: Logger,
) : UserRepository by delegate {
    // 只覆盖需要添加日志的内容
    override suspend fun findById(id: String): User? {
        logger.info("Finding user by id: $id")
        return delegate.findById(id).also {
            logger.info("Found user: ${it?.name ?: "null"}")
        }
    }
}
```

## DSL 构建器

### 类型安全构建器

```kotlin
// 好：带 @DslMarker 的 DSL
@DslMarker
annotation class HtmlDsl

@HtmlDsl
class HTML {
    private val children = mutableListOf<Element>()

    fun head(init: Head.() -> Unit) {
        children += Head().apply(init)
    }

    fun body(init: Body.() -> Unit) {
        children += Body().apply(init)
    }

    override fun toString(): String = children.joinToString("\n")
}

fun html(init: HTML.() -> Unit): HTML = HTML().apply(init)

// 使用
val page = html {
    head { title("My Page") }
    body {
        h1("Welcome")
        p("Hello, World!")
    }
}
```

### 配置 DSL

```kotlin
data class ServerConfig(
    val host: String = "0.0.0.0",
    val port: Int = 8080,
    val ssl: SslConfig? = null,
    val database: DatabaseConfig? = null,
)

data class SslConfig(val certPath: String, val keyPath: String)
data class DatabaseConfig(val url: String, val maxPoolSize: Int = 10)

class ServerConfigBuilder {
    var host: String = "0.0.0.0"
    var port: Int = 8080
    private var ssl: SslConfig? = null
    private var database: DatabaseConfig? = null

    fun ssl(certPath: String, keyPath: String) {
        ssl = SslConfig(certPath, keyPath)
    }

    fun database(url: String, maxPoolSize: Int = 10) {
        database = DatabaseConfig(url, maxPoolSize)
    }

    fun build(): ServerConfig = ServerConfig(host, port, ssl, database)
}

fun serverConfig(init: ServerConfigBuilder.() -> Unit): ServerConfig =
    ServerConfigBuilder().apply(init).build()

// 使用
val config = serverConfig {
    host = "0.0.0.0"
    port = 443
    ssl("/certs/cert.pem", "/certs/key.pem")
    database("jdbc:postgresql://localhost:5432/mydb", maxPoolSize = 20)
}
```

## 用于惰性求值的序列

```kotlin
// 好：对具有多个操作的大型集合使用序列
val result = users.asSequence()
    .filter { it.isActive }
    .map { it.email }
    .filter { it.endsWith("@company.com") }
    .take(10)
    .toList()

// 好：生成无限序列
val fibonacci: Sequence<Long> = sequence {
    var a = 0L
    var b = 1L
    while (true) {
        yield(a)
        val next = a + b
        a = b
        b = next
    }
}

val first20 = fibonacci.take(20).toList()
```

## Gradle Kotlin DSL

### build.gradle.kts 配置

```kotlin
// 检查最新版本：https://kotlinlang.org/docs/releases.html
plugins {
    kotlin("jvm") version "2.3.10"
    kotlin("plugin.serialization") version "2.3.10"
    id("io.ktor.plugin") version "3.4.0"
    id("org.jetbrains.kotlinx.kover") version "0.9.7"
    id("io.gitlab.arturbosch.detekt") version "1.23.8"
}

group = "com.example"
version = "1.0.0"

kotlin {
    jvmToolchain(21)
}

dependencies {
    // Ktor
    implementation("io.ktor:ktor-server-core:3.4.0")
    implementation("io.ktor:ktor-server-netty:3.4.0")
    implementation("io.ktor:ktor-server-content-negotiation:3.4.0")
    implementation("io.ktor:ktor-serialization-kotlinx-json:3.4.0")

    // Exposed
    implementation("org.jetbrains.exposed:exposed-core:1.0.0")
    implementation("org.jetbrains.exposed:exposed-dao:1.0.0")
    implementation("org.jetbrains.exposed:exposed-jdbc:1.0.0")
    implementation("org.jetbrains.exposed:exposed-kotlin-datetime:1.0.0")

    // Koin
    implementation("io.insert-koin:koin-ktor:4.2.0")

    // Coroutines
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")

    // Testing
    testImplementation("io.kotest:kotest-runner-junit5:6.1.4")
    testImplementation("io.kotest:kotest-assertions-core:6.1.4")
    testImplementation("io.kotest:kotest-property:6.1.4")
    testImplementation("io.mockk:mockk:1.14.9")
    testImplementation("io.ktor:ktor-server-test-host:3.4.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
}

tasks.withType<Test> {
    useJUnitPlatform()
}

detekt {
    config.setFrom(files("config/detekt/detekt.yml"))
    buildUponDefaultConfig = true
}
```

## 错误处理模式

### 用于领域操作的 Result 类型

```kotlin
// 好：使用 Kotlin 的 Result 或自定义密封类
suspend fun createUser(request: CreateUserRequest): Result<User> = runCatching {
    require(request.name.isNotBlank()) { "Name cannot be blank" }
    require('@' in request.email) { "Invalid email format" }

    val user = User(
        id = UserId(UUID.randomUUID().toString()),
        name = request.name,
        email = Email(request.email),
    )
    userRepository.save(user)
    user
}

// 好：链式结果
val displayName = createUser(request)
    .map { it.name }
    .getOrElse { "Unknown" }
```

### require、check、error

```kotlin
// 好：带清晰消息的前置条件
fun withdraw(account: Account, amount: Money): Account {
    require(amount.value > 0) { "Amount must be positive: $amount" }
    check(account.balance >= amount) { "Insufficient balance: ${account.balance} < $amount" }

    return account.copy(balance = account.balance - amount)
}
```

## 集合操作

### 惯用集合处理

```kotlin
// 好：链式操作
val activeAdminEmails: List<String> = users
    .filter { it.role == Role.ADMIN && it.isActive }
    .sortedBy { it.name }
    .map { it.email }

// 好：分组和聚合
val usersByRole: Map<Role, List<User>> = users.groupBy { it.role }

val oldestByRole: Map<Role, User?> = users.groupBy { it.role }
    .mapValues { (_, users) -> users.minByOrNull { it.createdAt } }

// 好：associate 用于创建映射
val usersById: Map<UserId, User> = users.associateBy { it.id }

// 好：Partition 用于分割
val (active, inactive) = users.partition { it.isActive }
```

## 快速参考：Kotlin 惯用语法

| 惯用语法 | 描述 |
|-------|-------------|
| `val` over `var` | 优先不可变变量 |
| `data class` | 用于带 equals/hashCode/copy 的值对象 |
| `sealed class/interface` | 用于受限类型层次结构 |
| `value class` | 用于零开销的类型安全包装器 |
| Expression `when` | 穷举模式匹配 |
| Safe call `?.` | 空安全成员访问 |
| Elvis `?:` | 可空的默认值 |
| `let`/`apply`/`also`/`run`/`with` | 用于清晰代码的作用域函数 |
| Extension functions | 不继承而添加行为 |
| `copy()` | 数据类的不可变更新 |
| `require`/`check` | 前置条件断言 |
| Coroutine `async`/`await` | 结构化并发执行 |
| `Flow` | 冷响应式流 |
| `sequence` | 惰性求值 |
| Delegation `by` | 不继承而重用实现 |

## 应避免的反模式

```kotlin
// 坏：强制解包可空类型
val name = user!!.name

// 坏：来自 Java 的平台类型泄漏
fun getLength(s: String) = s.length // 安全
fun getLength(s: String?) = s?.length ?: 0 // 处理来自 Java 的 null

// 坏：可变数据类
data class MutableUser(var name: String, var email: String)

// 坏：使用异常进行控制流
try {
    val user = findUser(id)
} catch (e: NotFoundException) {
    // 不要对预期情况使用异常
}

// 好：使用可空返回或 Result
val user: User? = findUserOrNull(id)

// 坏：忽略协程作用域
GlobalScope.launch { /* 避免 GlobalScope */ }

// 好：使用结构化并发
coroutineScope {
    launch { /* 正确作用域 */ }
}

// 坏：深层嵌套作用域函数
user?.let { u ->
    u.address?.let { a ->
        a.city?.let { c -> process(c) }
    }
}

// 好：直接空安全链
user?.address?.city?.let { process(it) }
```

**记住**：Kotlin 代码应该简洁但可读。利用类型系统保证安全，优先不可变性，使用协程进行并发。当有疑问时，让编译器帮助你。

---

---
name: kotlin-testing
description: 使用 Kotest、MockK、协程测试、属性测试和 Kover 覆盖率的 Kotlin 测试模式。遵循 TDD 方法论与惯用 Kotlin 实践。
origin: ECC
---

# Kotlin 测试模式

使用 Kotest 和 MockK 编写可靠、可维护测试的综合 Kotlin 测试模式，遵循 TDD 方法论。

## 何时使用

- 编写新的 Kotlin 函数或类
- 为现有 Kotlin 代码添加测试覆盖
- 实现属性测试
- 在 Kotlin 项目中遵循 TDD 工作流
- 配置 Kover 用于代码覆盖

## 工作原理

1. **识别目标代码** — 找到要测试的函数、类或模块
2. **编写 Kotest spec** — 选择匹配测试范围的 spec 样式（StringSpec、FunSpec、BehaviorSpec）
3. **Mock 依赖** — 使用 MockK 隔离被测单元
4. **运行测试（RED）** — 验证测试以预期错误失败
5. **实现代码（GREEN）** — 编写最小代码通过测试
6. **重构** — 在保持测试绿色时改进实现
7. **检查覆盖** — 运行 `./gradlew koverHtmlReport` 并验证 80%+ 覆盖

## 示例

以下部分包含每个测试模式的详细、可运行示例：

### 快速参考

- **Kotest specs** — StringSpec、FunSpec、BehaviorSpec 示例见 [Kotest Spec 样式](#kotest-spec-styles)
- **Mocking** — MockK 设置、协程 mock、参数捕获见 [MockK](#mockk)
- **TDD 演练** — EmailValidator 的完整 RED/GREEN/REFACTOR 循环见 [Kotlin TDD 工作流](#tdd-workflow-for-kotlin)
- **覆盖** — Kover 配置和命令见 [Kover 覆盖](#kover-coverage)
- **Ktor 测试** — testApplication 设置见 [Ktor testApplication 测试](#ktor-testapplication-testing)

### Kotlin TDD 工作流

#### RED-GREEN-REFACTOR 循环

```
RED     -> 先写一个失败的测试
GREEN   -> 编写最小代码通过测试
REFACTOR -> 在保持测试绿色时改进代码
REPEAT  -> 继续下一个需求
```

#### Kotlin 中的逐步 TDD

```kotlin
// 步骤 1：定义接口/签名
// EmailValidator.kt
package com.example.validator

fun validateEmail(email: String): Result<String> {
    TODO("not implemented")
}

// 步骤 2：写失败的测试（RED）
// EmailValidatorTest.kt
package com.example.validator

import io.kotest.core.spec.style.StringSpec
import io.kotest.matchers.result.shouldBeFailure
import io.kotest.matchers.result.shouldBeSuccess

class EmailValidatorTest : StringSpec({
    "valid email returns success" {
        validateEmail("user@example.com").shouldBeSuccess("user@example.com")
    }

    "empty email returns failure" {
        validateEmail("").shouldBeFailure()
    }

    "email without @ returns failure" {
        validateEmail("userexample.com").shouldBeFailure()
    }
})

// 步骤 3：运行测试 - 验证 FAIL
// $ ./gradlew test
// EmailValidatorTest > valid email returns success FAILED
//   kotlin.NotImplementedError: An operation is not implemented

// 步骤 4：实现最小代码（GREEN）
fun validateEmail(email: String): Result<String> {
    if (email.isBlank()) return Result.failure(IllegalArgumentException("Email cannot be blank"))
    if ('@' !in email) return Result.failure(IllegalArgumentException("Email must contain @"))
    val regex = Regex("^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}$")
    if (!regex.matches(email)) return Result.failure(IllegalArgumentException("Invalid email format"))
    return Result.success(email)
}

// 步骤 5：运行测试 - 验证 PASS
// $ ./gradlew test
// EmailValidatorTest > valid email returns success PASSED
// EmailValidatorTest > empty email returns failure PASSED
// EmailValidatorTest > email without @ returns failure PASSED

// 步骤 6：如需要则重构，验证测试仍通过
```

### Kotest Spec 样式

#### StringSpec（最简单）

```kotlin
class CalculatorTest : StringSpec({
    "add two positive numbers" {
        Calculator.add(2, 3) shouldBe 5
    }

    "add negative numbers" {
        Calculator.add(-1, -2) shouldBe -3
    }

    "add zero" {
        Calculator.add(0, 5) shouldBe 5
    }
})
```

#### FunSpec（JUnit 风格）

```kotlin
class UserServiceTest : FunSpec({
    val repository = mockk<UserRepository>()
    val service = UserService(repository)

    test("getUser returns user when found") {
        val expected = User(id = "1", name = "Alice")
        coEvery { repository.findById("1") } returns expected

        val result = service.getUser("1")

        result shouldBe expected
    }

    test("getUser throws when not found") {
        coEvery { repository.findById("999") } returns null

        shouldThrow<UserNotFoundException> {
            service.getUser("999")
        }
    }
})
```

#### BehaviorSpec（BDD 风格）

```kotlin
class OrderServiceTest : BehaviorSpec({
    val repository = mockk<OrderRepository>()
    val paymentService = mockk<PaymentService>()
    val service = OrderService(repository, paymentService)

    Given("a valid order request") {
        val request = CreateOrderRequest(
            userId = "user-1",
            items = listOf(OrderItem("product-1", quantity = 2)),
        )

        When("the order is placed") {
            coEvery { paymentService.charge(any()) } returns PaymentResult.Success
            coEvery { repository.save(any()) } answers { firstArg() }

            val result = service.placeOrder(request)

            Then("it should return a confirmed order") {
                result.status shouldBe OrderStatus.CONFIRMED
            }

            Then("it should charge payment") {
                coVerify(exactly = 1) { paymentService.charge(any()) }
            }
        }

        When("payment fails") {
            coEvery { paymentService.charge(any()) } returns PaymentResult.Declined

            Then("it should throw PaymentException") {
                shouldThrow<PaymentException> {
                    service.placeOrder(request)
                }
            }
        }
    }
})
```

#### DescribeSpec（RSpec 风格）

```kotlin
class UserValidatorTest : DescribeSpec({
    describe("validateUser") {
        val validator = UserValidator()

        context("with valid input") {
            it("accepts a normal user") {
                val user = CreateUserRequest("Alice", "alice@example.com")
                validator.validate(user).shouldBeValid()
            }
        }

        context("with invalid name") {
            it("rejects blank name") {
                val user = CreateUserRequest("", "alice@example.com")
                validator.validate(user).shouldBeInvalid()
            }

            it("rejects name exceeding max length") {
                val user = CreateUserRequest("A".repeat(256), "alice@example.com")
                validator.validate(user).shouldBeInvalid()
            }
        }
    }
})
```

### Kotest 匹配器

#### 核心匹配器

```kotlin
import io.kotest.matchers.shouldBe
import io.kotest.matchers.shouldNotBe
import io.kotest.matchers.string.*
import io.kotest.matchers.collections.*
import io.kotest.matchers.nulls.*

// 相等
result shouldBe expected
result shouldNotBe unexpected

// 字符串
name shouldStartWith "Al"
name shouldEndWith "ice"
name shouldContain "lic"
name shouldMatch Regex("[A-Z][a-z]+")
name.shouldBeBlank()

// 集合
list shouldContain "item"
list shouldHaveSize 3
list.shouldBeSorted()
list.shouldContainAll("a", "b", "c")
list.shouldBeEmpty()

// 空
result.shouldNotBeNull()
result.shouldBeNull()

// 类型
result.shouldBeInstanceOf<User>()

// 数字
count shouldBeGreaterThan 0
price shouldBeInRange 1.0..100.0

// 异常
shouldThrow<IllegalArgumentException> {
    validateAge(-1)
}.message shouldBe "Age must be positive"

shouldNotThrow<Exception> {
    validateAge(25)
}
```

#### 自定义匹配器

```kotlin
fun beActiveUser() = object : Matcher<User> {
    override fun test(value: User) = MatcherResult(
        value.isActive && value.lastLogin != null,
        { "User ${value.id} should be active with a last login" },
        { "User ${value.id} should not be active" },
    )
}
```

### MockK

#### 基本 Mock 设置

```kotlin
val repository = mockk<UserRepository>()
val service = UserService(repository)

// 每次调用返回固定值
every { repository.findById(any()) } returns null

// 使用 coEvery mock 挂起函数
coEvery { repository.findById("1") } returns User(id = "1", name = "Alice")

// 验证调用
verify { repository.findById("1") }
verify(exactly = 1) { repository.findAll() }
verify(inverse = true) { repository.delete(any()) }
```

#### 协程 Mock

```kotlin
coEvery { repository.save(any()) } returns Unit
coEvery { repository.findById(any()) } returns null

// 抛出异常
coEvery { repository.findById("error") } throws RuntimeException("Database error")

// 延迟响应
coEvery { repository.findById(any()) } coAnswers {
    delay(100)
    User(id = "1", name = "Test")
}
```

#### 参数捕获

```kotlin
val slot = slot<User>()
coEvery { repository.save(capture(slot)) } returns Unit

service.createUser(User(name = "Alice"))
assert(slot.captured.name == "Alice")
```

#### MockK 最佳实践

- 使用 `mockk< T >()` 创建 mock
- 对挂起函数使用 `coEvery` 和 `coVerify`
- 使用 `any()` 匹配任意参数
- 使用 `slot()` 捕获参数值
- 每个测试创建新 mock 以避免状态泄漏

## Kover 覆盖

### Gradle 配置

```kotlin
plugins {
    id("org.jetbrains.kotlinx.kover") version "0.9.7"
}

kover {
    reports {
        xml.required.set(true)
        html.required.set(true)
    }
}
```

### 运行覆盖报告

```bash
./gradlew koverHtmlReport     # HTML 报告
./gradlew koverXmlReport       # XML 报告（CI 兼容）
./gradlew koverVerify         # 验证覆盖门槛
```

### 覆盖门槛

```kotlin
kover {
    bounds {
        minBound(LineCoverage(80))           // 至少 80% 行覆盖
        minBound(BranchCoverage(70))        // 至少 70% 分支覆盖
    }
}
```

## Ktor testApplication 测试

```kotlin
class UserRouteTest : DescribeSpec({
    describe("User routes") {
        testApplication {
            val client = createClient()
            val userService = mockk<UserService>()
            install(Routing) {
                userRoutes(userService)
            }

            coEvery { userService.getUser(any()) } returns User(id = "1", name = "Alice")

            val response = client.get("/users/1")

            response.status().shouldBe(HttpStatusCode.OK)
            response.bodyAsText().shouldContain("Alice")
        }
    }
})
```

## 属性测试

```kotlin
import io.kotest.property.forAll

class EmailPropertyTest : StringSpec({
    "valid email formats are accepted" {
        forAll(
            Gen.string(minSize = 5).suchThat { '@' in it && it.count { c -> c == '@' } == 1 },
        ) { email ->
            validateEmail(email).isSuccess
        }
    }
})
```

## 集成测试

```kotlin
class UserRepositoryIntegrationTest : StringSpec({
    val database = Database.connect("jdbc:h2:mem:test")
    
    "user can be saved and retrieved" {
        val repository = JdbcUserRepository(database)
        val user = User(id = "1", name = "Alice", email = "alice@example.com")
        
        repository.save(user)
        val found = repository.findById("1")
        
        found.shouldNotBeNull()
        found!!.name shouldBe "Alice"
    }
})
```

## 测试最佳实践

- **每个测试一个关注点** — 保持测试简短和专注
- **AAA 模式** — Arrange（准备）、Act（行动）、Assert（断言）
- **描述性测试名称** — 测试名称应描述预期行为
- **隔离** — 每个测试独立运行，无隐藏依赖
- **快速** — 单元测试应毫秒级运行
- **确定性** — 无随机失败，无依赖时间
- **Mock 外部依赖** — 不连接真实数据库或 API