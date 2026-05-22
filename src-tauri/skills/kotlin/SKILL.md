---
name: kotlin-patterns
description: Idiomatic Kotlin patterns, best practices, and conventions for building robust, efficient, and maintainable Kotlin applications with coroutines, null safety, and DSL builders.
origin: ECC
---

# Kotlin Development Patterns

Idiomatic Kotlin patterns and best practices for building robust, efficient, and maintainable applications.

## When to Use

- Writing new Kotlin code
- Reviewing Kotlin code
- Refactoring existing Kotlin code
- Designing Kotlin modules or libraries
- Configuring Gradle Kotlin DSL builds

## How It Works

This skill enforces idiomatic Kotlin conventions across seven key areas: null safety using the type system and safe-call operators, immutability via `val` and `copy()` on data classes, sealed classes and interfaces for exhaustive type hierarchies, structured concurrency with coroutines and `Flow`, extension functions for adding behaviour without inheritance, type-safe DSL builders using `@DslMarker` and lambda receivers, and Gradle Kotlin DSL for build configuration.

## Examples

**Null safety with Elvis operator:**
```kotlin
fun getUserEmail(userId: String): String {
    val user = userRepository.findById(userId)
    return user?.email ?: "unknown@example.com"
}
```

**Sealed class for exhaustive results:**
```kotlin
sealed class Result<out T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Failure(val error: AppError) : Result<Nothing>()
    data object Loading : Result<Nothing>()
}
```

**Structured concurrency with async/await:**
```kotlin
suspend fun fetchUserWithPosts(userId: String): UserProfile =
    coroutineScope {
        val user = async { userService.getUser(userId) }
        val posts = async { postService.getUserPosts(userId) }
        UserProfile(user = user.await(), posts = posts.await())
    }
```

## Core Principles

### 1. Null Safety

Kotlin's type system distinguishes nullable and non-nullable types. Leverage it fully.

```kotlin
// Good: Use non-nullable types by default
fun getUser(id: String): User {
    return userRepository.findById(id)
        ?: throw UserNotFoundException("User $id not found")
}

// Good: Safe calls and Elvis operator
fun getUserEmail(userId: String): String {
    val user = userRepository.findById(userId)
    return user?.email ?: "unknown@example.com"
}

// Bad: Force-unwrapping nullable types
fun getUserEmail(userId: String): String {
    val user = userRepository.findById(userId)
    return user!!.email // Throws NPE if null
}
```

### 2. Immutability by Default

Prefer `val` over `var`, immutable collections over mutable ones.

```kotlin
// Good: Immutable data
data class User(
    val id: String,
    val name: String,
    val email: String,
)

// Good: Transform with copy()
fun updateEmail(user: User, newEmail: String): User =
    user.copy(email = newEmail)

// Good: Immutable collections
val users: List<User> = listOf(user1, user2)
val filtered = users.filter { it.email.isNotBlank() }

// Bad: Mutable state
var currentUser: User? = null // Avoid mutable global state
val mutableUsers = mutableListOf<User>() // Avoid unless truly needed
```

### 3. Expression Bodies and Single-Expression Functions

Use expression bodies for concise, readable functions.

```kotlin
// Good: Expression body
fun isAdult(age: Int): Boolean = age >= 18

fun formatFullName(first: String, last: String): String =
    "$first $last".trim()

fun User.displayName(): String =
    name.ifBlank { email.substringBefore('@') }

// Good: When as expression
fun statusMessage(code: Int): String = when (code) {
    200 -> "OK"
    404 -> "Not Found"
    500 -> "Internal Server Error"
    else -> "Unknown status: $code"
}

// Bad: Unnecessary block body
fun isAdult(age: Int): Boolean {
    return age >= 18
}
```

### 4. Data Classes for Value Objects

Use data classes for types that primarily hold data.

```kotlin
// Good: Data class with copy, equals, hashCode, toString
data class CreateUserRequest(
    val name: String,
    val email: String,
    val role: Role = Role.USER,
)

// Good: Value class for type safety (zero overhead at runtime)
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

## Sealed Classes and Interfaces

### Modeling Restricted Hierarchies

```kotlin
// Good: Sealed class for exhaustive when
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

### Sealed Interfaces for API Responses

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

## Scope Functions

### When to Use Each

```kotlin
// let: Transform nullable or scoped result
val length: Int? = name?.let { it.trim().length }

// apply: Configure an object (returns the object)
val user = User().apply {
    name = "Alice"
    email = "alice@example.com"
}

// also: Side effects (returns the object)
val user = createUser(request).also { logger.info("Created user: ${it.id}") }

// run: Execute a block with receiver (returns result)
val result = connection.run {
    prepareStatement(sql)
    executeQuery()
}

// with: Non-extension form of run
val csv = with(StringBuilder()) {
    appendLine("name,email")
    users.forEach { appendLine("${it.name},${it.email}") }
    toString()
}
```

### Anti-Patterns

```kotlin
// Bad: Nesting scope functions
user?.let { u ->
    u.address?.let { addr ->
        addr.city?.let { city ->
            println(city) // Hard to read
        }
    }
}

// Good: Chain safe calls instead
val city = user?.address?.city
city?.let { println(it) }
```

## Extension Functions

### Adding Functionality Without Inheritance

```kotlin
// Good: Domain-specific extensions
fun String.toSlug(): String =
    lowercase()
        .replace(Regex("[^a-z0-9\\s-]"), "")
        .replace(Regex("\\s+"), "-")
        .trim('-')

fun Instant.toLocalDate(zone: ZoneId = ZoneId.systemDefault()): LocalDate =
    atZone(zone).toLocalDate()

// Good: Collection extensions
fun <T> List<T>.second(): T = this[1]

fun <T> List<T>.secondOrNull(): T? = getOrNull(1)

// Good: Scoped extensions (not polluting global namespace)
class UserService {
    private fun User.isActive(): Boolean =
        status == Status.ACTIVE && lastLogin.isAfter(Instant.now().minus(30, ChronoUnit.DAYS))

    fun getActiveUsers(): List<User> = userRepository.findAll().filter { it.isActive() }
}
```

## Coroutines

### Structured Concurrency

```kotlin
// Good: Structured concurrency with coroutineScope
suspend fun fetchUserWithPosts(userId: String): UserProfile =
    coroutineScope {
        val userDeferred = async { userService.getUser(userId) }
        val postsDeferred = async { postService.getUserPosts(userId) }

        UserProfile(
            user = userDeferred.await(),
            posts = postsDeferred.await(),
        )
    }

// Good: supervisorScope when children can fail independently
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

### Flow for Reactive Streams

```kotlin
// Good: Cold flow with proper error handling
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

// Good: Flow operators
fun searchUsers(query: Flow<String>): Flow<List<User>> =
    query
        .debounce(300.milliseconds)
        .distinctUntilChanged()
        .filter { it.length >= 2 }
        .mapLatest { q -> userRepository.search(q) }
        .catch { emit(emptyList()) }
```

### Cancellation and Cleanup

```kotlin
// Good: Respect cancellation
suspend fun processItems(items: List<Item>) {
    items.forEach { item ->
        ensureActive() // Check cancellation before expensive work
        processItem(item)
    }
}

// Good: Cleanup with try/finally
suspend fun acquireAndProcess() {
    val resource = acquireResource()
    try {
        resource.process()
    } finally {
        withContext(NonCancellable) {
            resource.release() // Always release, even on cancellation
        }
    }
}
```

## Delegation

### Property Delegation

```kotlin
// Lazy initialization
val expensiveData: List<User> by lazy {
    userRepository.findAll()
}

// Observable property
var name: String by Delegates.observable("initial") { _, old, new ->
    logger.info("Name changed from '$old' to '$new'")
}

// Map-backed properties
class Config(private val map: Map<String, Any?>) {
    val host: String by map
    val port: Int by map
    val debug: Boolean by map
}

val config = Config(mapOf("host" to "localhost", "port" to 8080, "debug" to true))
```

### Interface Delegation

```kotlin
// Good: Delegate interface implementation
class LoggingUserRepository(
    private val delegate: UserRepository,
    private val logger: Logger,
) : UserRepository by delegate {
    // Only override what you need to add logging to
    override suspend fun findById(id: String): User? {
        logger.info("Finding user by id: $id")
        return delegate.findById(id).also {
            logger.info("Found user: ${it?.name ?: "null"}")
        }
    }
}
```

## DSL Builders

### Type-Safe Builders

```kotlin
// Good: DSL with @DslMarker
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

// Usage
val page = html {
    head { title("My Page") }
    body {
        h1("Welcome")
        p("Hello, World!")
    }
}
```

### Configuration DSL

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

// Usage
val config = serverConfig {
    host = "0.0.0.0"
    port = 443
    ssl("/certs/cert.pem", "/certs/key.pem")
    database("jdbc:postgresql://localhost:5432/mydb", maxPoolSize = 20)
}
```

## Sequences for Lazy Evaluation

```kotlin
// Good: Use sequences for large collections with multiple operations
val result = users.asSequence()
    .filter { it.isActive }
    .map { it.email }
    .filter { it.endsWith("@company.com") }
    .take(10)
    .toList()

// Good: Generate infinite sequences
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

### build.gradle.kts Configuration

```kotlin
// Check for latest versions: https://kotlinlang.org/docs/releases.html
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

## Error Handling Patterns

### Result Type for Domain Operations

```kotlin
// Good: Use Kotlin's Result or a custom sealed class
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

// Good: Chain results
val displayName = createUser(request)
    .map { it.name }
    .getOrElse { "Unknown" }
```

### require, check, error

```kotlin
// Good: Preconditions with clear messages
fun withdraw(account: Account, amount: Money): Account {
    require(amount.value > 0) { "Amount must be positive: $amount" }
    check(account.balance >= amount) { "Insufficient balance: ${account.balance} < $amount" }

    return account.copy(balance = account.balance - amount)
}
```

## Collection Operations

### Idiomatic Collection Processing

```kotlin
// Good: Chained operations
val activeAdminEmails: List<String> = users
    .filter { it.role == Role.ADMIN && it.isActive }
    .sortedBy { it.name }
    .map { it.email }

// Good: Grouping and aggregation
val usersByRole: Map<Role, List<User>> = users.groupBy { it.role }

val oldestByRole: Map<Role, User?> = users.groupBy { it.role }
    .mapValues { (_, users) -> users.minByOrNull { it.createdAt } }

// Good: Associate for map creation
val usersById: Map<UserId, User> = users.associateBy { it.id }

// Good: Partition for splitting
val (active, inactive) = users.partition { it.isActive }
```

## Quick Reference: Kotlin Idioms

| Idiom | Description |
|-------|-------------|
| `val` over `var` | Prefer immutable variables |
| `data class` | For value objects with equals/hashCode/copy |
| `sealed class/interface` | For restricted type hierarchies |
| `value class` | For type-safe wrappers with zero overhead |
| Expression `when` | Exhaustive pattern matching |
| Safe call `?.` | Null-safe member access |
| Elvis `?:` | Default value for nullables |
| `let`/`apply`/`also`/`run`/`with` | Scope functions for clean code |
| Extension functions | Add behavior without inheritance |
| `copy()` | Immutable updates on data classes |
| `require`/`check` | Precondition assertions |
| Coroutine `async`/`await` | Structured concurrent execution |
| `Flow` | Cold reactive streams |
| `sequence` | Lazy evaluation |
| Delegation `by` | Reuse implementation without inheritance |

## Anti-Patterns to Avoid

```kotlin
// Bad: Force-unwrapping nullable types
val name = user!!.name

// Bad: Platform type leakage from Java
fun getLength(s: String) = s.length // Safe
fun getLength(s: String?) = s?.length ?: 0 // Handle nulls from Java

// Bad: Mutable data classes
data class MutableUser(var name: String, var email: String)

// Bad: Using exceptions for control flow
try {
    val user = findUser(id)
} catch (e: NotFoundException) {
    // Don't use exceptions for expected cases
}

// Good: Use nullable return or Result
val user: User? = findUserOrNull(id)

// Bad: Ignoring coroutine scope
GlobalScope.launch { /* Avoid GlobalScope */ }

// Good: Use structured concurrency
coroutineScope {
    launch { /* Properly scoped */ }
}

// Bad: Deeply nested scope functions
user?.let { u ->
    u.address?.let { a ->
        a.city?.let { c -> process(c) }
    }
}

// Good: Direct null-safe chain
user?.address?.city?.let { process(it) }
```

**Remember**: Kotlin code should be concise but readable. Leverage the type system for safety, prefer immutability, and use coroutines for concurrency. When in doubt, let the compiler help you.


---

---
name: kotlin-testing
description: Kotlin testing patterns with Kotest, MockK, coroutine testing, property-based testing, and Kover coverage. Follows TDD methodology with idiomatic Kotlin practices.
origin: ECC
---

# Kotlin Testing Patterns

Comprehensive Kotlin testing patterns for writing reliable, maintainable tests following TDD methodology with Kotest and MockK.

## When to Use

- Writing new Kotlin functions or classes
- Adding test coverage to existing Kotlin code
- Implementing property-based tests
- Following TDD workflow in Kotlin projects
- Configuring Kover for code coverage

## How It Works

1. **Identify target code** — Find the function, class, or module to test
2. **Write a Kotest spec** — Choose a spec style (StringSpec, FunSpec, BehaviorSpec) matching the test scope
3. **Mock dependencies** — Use MockK to isolate the unit under test
4. **Run tests (RED)** — Verify the test fails with the expected error
5. **Implement code (GREEN)** — Write minimal code to pass the test
6. **Refactor** — Improve the implementation while keeping tests green
7. **Check coverage** — Run `./gradlew koverHtmlReport` and verify 80%+ coverage

## Examples

The following sections contain detailed, runnable examples for each testing pattern:

### Quick Reference

- **Kotest specs** — StringSpec, FunSpec, BehaviorSpec, DescribeSpec examples in [Kotest Spec Styles](#kotest-spec-styles)
- **Mocking** — MockK setup, coroutine mocking, argument capture in [MockK](#mockk)
- **TDD walkthrough** — Full RED/GREEN/REFACTOR cycle with EmailValidator in [TDD Workflow for Kotlin](#tdd-workflow-for-kotlin)
- **Coverage** — Kover configuration and commands in [Kover Coverage](#kover-coverage)
- **Ktor testing** — testApplication setup in [Ktor testApplication Testing](#ktor-testapplication-testing)

### TDD Workflow for Kotlin

#### The RED-GREEN-REFACTOR Cycle

```
RED     -> Write a failing test first
GREEN   -> Write minimal code to pass the test
REFACTOR -> Improve code while keeping tests green
REPEAT  -> Continue with next requirement
```

#### Step-by-Step TDD in Kotlin

```kotlin
// Step 1: Define the interface/signature
// EmailValidator.kt
package com.example.validator

fun validateEmail(email: String): Result<String> {
    TODO("not implemented")
}

// Step 2: Write failing test (RED)
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

// Step 3: Run tests - verify FAIL
// $ ./gradlew test
// EmailValidatorTest > valid email returns success FAILED
//   kotlin.NotImplementedError: An operation is not implemented

// Step 4: Implement minimal code (GREEN)
fun validateEmail(email: String): Result<String> {
    if (email.isBlank()) return Result.failure(IllegalArgumentException("Email cannot be blank"))
    if ('@' !in email) return Result.failure(IllegalArgumentException("Email must contain @"))
    val regex = Regex("^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}$")
    if (!regex.matches(email)) return Result.failure(IllegalArgumentException("Invalid email format"))
    return Result.success(email)
}

// Step 5: Run tests - verify PASS
// $ ./gradlew test
// EmailValidatorTest > valid email returns success PASSED
// EmailValidatorTest > empty email returns failure PASSED
// EmailValidatorTest > email without @ returns failure PASSED

// Step 6: Refactor if needed, verify tests still pass
```

### Kotest Spec Styles

#### StringSpec (Simplest)

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

#### FunSpec (JUnit-like)

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

#### BehaviorSpec (BDD Style)

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

#### DescribeSpec (RSpec Style)

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

### Kotest Matchers

#### Core Matchers

```kotlin
import io.kotest.matchers.shouldBe
import io.kotest.matchers.shouldNotBe
import io.kotest.matchers.string.*
import io.kotest.matchers.collections.*
import io.kotest.matchers.nulls.*

// Equality
result shouldBe expected
result shouldNotBe unexpected

// Strings
name shouldStartWith "Al"
name shouldEndWith "ice"
name shouldContain "lic"
name shouldMatch Regex("[A-Z][a-z]+")
name.shouldBeBlank()

// Collections
list shouldContain "item"
list shouldHaveSize 3
list.shouldBeSorted()
list.shouldContainAll("a", "b", "c")
list.shouldBeEmpty()

// Nulls
result.shouldNotBeNull()
result.shouldBeNull()

// Types
result.shouldBeInstanceOf<User>()

// Numbers
count shouldBeGreaterThan 0
price shouldBeInRange 1.0..100.0

// Exceptions
shouldThrow<IllegalArgumentException> {
    validateAge(-1)
}.message shouldBe "Age must be positive"

shouldNotThrow<Exception> {
    validateAge(25)
}
```

#### Custom Matchers

```kotlin
fun beActiveUser() = object : Matcher<User> {
    override fun test(value: User) = MatcherResult(
        value.isActive && value.lastLogin != null,
        { "User ${value.id} should be active with a last login" },
        { "User ${value.id} should not be active" },
    )
}

// Usage
user should beActiveUser()
```

### MockK

#### Basic Mocking

```kotlin
class UserServiceTest : FunSpec({
    val repository = mockk<UserRepository>()
    val logger = mockk<Logger>(relaxed = true) // Relaxed: returns defaults
    val service = UserService(repository, logger)

    beforeTest {
        clearMocks(repository, logger)
    }

    test("findUser delegates to repository") {
        val expected = User(id = "1", name = "Alice")
        every { repository.findById("1") } returns expected

        val result = service.findUser("1")

        result shouldBe expected
        verify(exactly = 1) { repository.findById("1") }
    }

    test("findUser returns null for unknown id") {
        every { repository.findById(any()) } returns null

        val result = service.findUser("unknown")

        result.shouldBeNull()
    }
})
```

#### Coroutine Mocking

```kotlin
class AsyncUserServiceTest : FunSpec({
    val repository = mockk<UserRepository>()
    val service = UserService(repository)

    test("getUser suspending function") {
        coEvery { repository.findById("1") } returns User(id = "1", name = "Alice")

        val result = service.getUser("1")

        result.name shouldBe "Alice"
        coVerify { repository.findById("1") }
    }

    test("getUser with delay") {
        coEvery { repository.findById("1") } coAnswers {
            delay(100) // Simulate async work
            User(id = "1", name = "Alice")
        }

        val result = service.getUser("1")
        result.name shouldBe "Alice"
    }
})
```

#### Argument Capture

```kotlin
test("save captures the user argument") {
    val slot = slot<User>()
    coEvery { repository.save(capture(slot)) } returns Unit

    service.createUser(CreateUserRequest("Alice", "alice@example.com"))

    slot.captured.name shouldBe "Alice"
    slot.captured.email shouldBe "alice@example.com"
    slot.captured.id.shouldNotBeNull()
}
```

#### Spy and Partial Mocking

```kotlin
test("spy on real object") {
    val realService = UserService(repository)
    val spy = spyk(realService)

    every { spy.generateId() } returns "fixed-id"

    spy.createUser(request)

    verify { spy.generateId() } // Overridden
    // Other methods use real implementation
}
```

### Coroutine Testing

#### runTest for Suspend Functions

```kotlin
import kotlinx.coroutines.test.runTest

class CoroutineServiceTest : FunSpec({
    test("concurrent fetches complete together") {
        runTest {
            val service = DataService(testScope = this)

            val result = service.fetchAllData()

            result.users.shouldNotBeEmpty()
            result.products.shouldNotBeEmpty()
        }
    }

    test("timeout after delay") {
        runTest {
            val service = SlowService()

            shouldThrow<TimeoutCancellationException> {
                withTimeout(100) {
                    service.slowOperation() // Takes > 100ms
                }
            }
        }
    }
})
```

#### Testing Flows

```kotlin
import io.kotest.matchers.collections.shouldContainInOrder
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest

class FlowServiceTest : FunSpec({
    test("observeUsers emits updates") {
        runTest {
            val service = UserFlowService()

            val emissions = service.observeUsers()
                .take(3)
                .toList()

            emissions shouldHaveSize 3
            emissions.last().shouldNotBeEmpty()
        }
    }

    test("searchUsers debounces input") {
        runTest {
            val service = SearchService()
            val queries = MutableSharedFlow<String>()

            val results = mutableListOf<List<User>>()
            val job = launch {
                service.searchUsers(queries).collect { results.add(it) }
            }

            queries.emit("a")
            queries.emit("ab")
            queries.emit("abc") // Only this should trigger search
            advanceTimeBy(500)

            results shouldHaveSize 1
            job.cancel()
        }
    }
})
```

#### TestDispatcher

```kotlin
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle

class DispatcherTest : FunSpec({
    test("uses test dispatcher for controlled execution") {
        val dispatcher = StandardTestDispatcher()

        runTest(dispatcher) {
            var completed = false

            launch {
                delay(1000)
                completed = true
            }

            completed shouldBe false
            advanceTimeBy(1000)
            completed shouldBe true
        }
    }
})
```

### Property-Based Testing

#### Kotest Property Testing

```kotlin
import io.kotest.core.spec.style.FunSpec
import io.kotest.property.Arb
import io.kotest.property.arbitrary.*
import io.kotest.property.forAll
import io.kotest.property.checkAll
import kotlinx.serialization.json.Json
import kotlinx.serialization.encodeToString
import kotlinx.serialization.decodeFromString

// Note: The serialization roundtrip test below requires the User data class
// to be annotated with @Serializable (from kotlinx.serialization).

class PropertyTest : FunSpec({
    test("string reverse is involutory") {
        forAll<String> { s ->
            s.reversed().reversed() == s
        }
    }

    test("list sort is idempotent") {
        forAll(Arb.list(Arb.int())) { list ->
            list.sorted() == list.sorted().sorted()
        }
    }

    test("serialization roundtrip preserves data") {
        checkAll(Arb.bind(Arb.string(1..50), Arb.string(5..100)) { name, email ->
            User(name = name, email = "$email@test.com")
        }) { user ->
            val json = Json.encodeToString(user)
            val decoded = Json.decodeFromString<User>(json)
            decoded shouldBe user
        }
    }
})
```

#### Custom Generators

```kotlin
val userArb: Arb<User> = Arb.bind(
    Arb.string(minSize = 1, maxSize = 50),
    Arb.email(),
    Arb.enum<Role>(),
) { name, email, role ->
    User(
        id = UserId(UUID.randomUUID().toString()),
        name = name,
        email = Email(email),
        role = role,
    )
}

val moneyArb: Arb<Money> = Arb.bind(
    Arb.long(1L..1_000_000L),
    Arb.enum<Currency>(),
) { amount, currency ->
    Money(amount, currency)
}
```

### Data-Driven Testing

#### withData in Kotest

```kotlin
class ParserTest : FunSpec({
    context("parsing valid dates") {
        withData(
            "2026-01-15" to LocalDate(2026, 1, 15),
            "2026-12-31" to LocalDate(2026, 12, 31),
            "2000-01-01" to LocalDate(2000, 1, 1),
        ) { (input, expected) ->
            parseDate(input) shouldBe expected
        }
    }

    context("rejecting invalid dates") {
        withData(
            nameFn = { "rejects '$it'" },
            "not-a-date",
            "2026-13-01",
            "2026-00-15",
            "",
        ) { input ->
            shouldThrow<DateParseException> {
                parseDate(input)
            }
        }
    }
})
```

### Test Lifecycle and Fixtures

#### BeforeTest / AfterTest

```kotlin
class DatabaseTest : FunSpec({
    lateinit var db: Database

    beforeSpec {
        db = Database.connect("jdbc:h2:mem:test;DB_CLOSE_DELAY=-1")
        transaction(db) {
            SchemaUtils.create(UsersTable)
        }
    }

    afterSpec {
        transaction(db) {
            SchemaUtils.drop(UsersTable)
        }
    }

    beforeTest {
        transaction(db) {
            UsersTable.deleteAll()
        }
    }

    test("insert and retrieve user") {
        transaction(db) {
            UsersTable.insert {
                it[name] = "Alice"
                it[email] = "alice@example.com"
            }
        }

        val users = transaction(db) {
            UsersTable.selectAll().map { it[UsersTable.name] }
        }

        users shouldContain "Alice"
    }
})
```

#### Kotest Extensions

```kotlin
// Reusable test extension
class DatabaseExtension : BeforeSpecListener, AfterSpecListener {
    lateinit var db: Database

    override suspend fun beforeSpec(spec: Spec) {
        db = Database.connect("jdbc:h2:mem:test;DB_CLOSE_DELAY=-1")
    }

    override suspend fun afterSpec(spec: Spec) {
        // cleanup
    }
}

class UserRepositoryTest : FunSpec({
    val dbExt = DatabaseExtension()
    register(dbExt)

    test("save and find user") {
        val repo = UserRepository(dbExt.db)
        // ...
    }
})
```

### Kover Coverage

#### Gradle Configuration

```kotlin
// build.gradle.kts
plugins {
    id("org.jetbrains.kotlinx.kover") version "0.9.7"
}

kover {
    reports {
        total {
            html { onCheck = true }
            xml { onCheck = true }
        }
        filters {
            excludes {
                classes("*.generated.*", "*.config.*")
            }
        }
        verify {
            rule {
                minBound(80) // Fail build below 80% coverage
            }
        }
    }
}
```

#### Coverage Commands

```bash
# Run tests with coverage
./gradlew koverHtmlReport

# Verify coverage thresholds
./gradlew koverVerify

# XML report for CI
./gradlew koverXmlReport

# View HTML report (use the command for your OS)
# macOS:   open build/reports/kover/html/index.html
# Linux:   xdg-open build/reports/kover/html/index.html
# Windows: start build/reports/kover/html/index.html
```

#### Coverage Targets

| Code Type | Target |
|-----------|--------|
| Critical business logic | 100% |
| Public APIs | 90%+ |
| General code | 80%+ |
| Generated / config code | Exclude |

### Ktor testApplication Testing

```kotlin
class ApiRoutesTest : FunSpec({
    test("GET /users returns list") {
        testApplication {
            application {
                configureRouting()
                configureSerialization()
            }

            val response = client.get("/users")

            response.status shouldBe HttpStatusCode.OK
            val users = response.body<List<UserResponse>>()
            users.shouldNotBeEmpty()
        }
    }

    test("POST /users creates user") {
        testApplication {
            application {
                configureRouting()
                configureSerialization()
            }

            val response = client.post("/users") {
                contentType(ContentType.Application.Json)
                setBody(CreateUserRequest("Alice", "alice@example.com"))
            }

            response.status shouldBe HttpStatusCode.Created
        }
    }
})
```

### Testing Commands

```bash
# Run all tests
./gradlew test

# Run specific test class
./gradlew test --tests "com.example.UserServiceTest"

# Run specific test
./gradlew test --tests "com.example.UserServiceTest.getUser returns user when found"

# Run with verbose output
./gradlew test --info

# Run with coverage
./gradlew koverHtmlReport

# Run detekt (static analysis)
./gradlew detekt

# Run ktlint (formatting check)
./gradlew ktlintCheck

# Continuous testing
./gradlew test --continuous
```

### Best Practices

**DO:**
- Write tests FIRST (TDD)
- Use Kotest's spec styles consistently across the project
- Use MockK's `coEvery`/`coVerify` for suspend functions
- Use `runTest` for coroutine testing
- Test behavior, not implementation
- Use property-based testing for pure functions
- Use `data class` test fixtures for clarity

**DON'T:**
- Mix testing frameworks (pick Kotest and stick with it)
- Mock data classes (use real instances)
- Use `Thread.sleep()` in coroutine tests (use `advanceTimeBy`)
- Skip the RED phase in TDD
- Test private functions directly
- Ignore flaky tests

### Integration with CI/CD

```yaml
# GitHub Actions example
test:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-java@v4
      with:
        distribution: 'temurin'
        java-version: '21'

    - name: Run tests with coverage
      run: ./gradlew test koverXmlReport

    - name: Verify coverage
      run: ./gradlew koverVerify

    - name: Upload coverage
      uses: codecov/codecov-action@v5
      with:
        files: build/reports/kover/report.xml
        token: ${{ secrets.CODECOV_TOKEN }}
```

**Remember**: Tests are documentation. They show how your Kotlin code is meant to be used. Use Kotest's expressive matchers to make tests readable and MockK for clean mocking of dependencies.


---

---
name: kotlin-coroutines-flows
description: Kotlin Coroutines and Flow patterns for Android and KMP — structured concurrency, Flow operators, StateFlow, error handling, and testing.
origin: ECC
---

# Kotlin Coroutines & Flows

Patterns for structured concurrency, Flow-based reactive streams, and coroutine testing in Android and Kotlin Multiplatform projects.

## When to Activate

- Writing async code with Kotlin coroutines
- Using Flow, StateFlow, or SharedFlow for reactive data
- Handling concurrent operations (parallel loading, debounce, retry)
- Testing coroutines and Flows
- Managing coroutine scopes and cancellation

## Structured Concurrency

### Scope Hierarchy

```
Application
  └── viewModelScope (ViewModel)
        └── coroutineScope { } (structured child)
              ├── async { } (concurrent task)
              └── async { } (concurrent task)
```

Always use structured concurrency — never `GlobalScope`:

```kotlin
// BAD
GlobalScope.launch { fetchData() }

// GOOD — scoped to ViewModel lifecycle
viewModelScope.launch { fetchData() }

// GOOD — scoped to composable lifecycle
LaunchedEffect(key) { fetchData() }
```

### Parallel Decomposition

Use `coroutineScope` + `async` for parallel work:

```kotlin
suspend fun loadDashboard(): Dashboard = coroutineScope {
    val items = async { itemRepository.getRecent() }
    val stats = async { statsRepository.getToday() }
    val profile = async { userRepository.getCurrent() }
    Dashboard(
        items = items.await(),
        stats = stats.await(),
        profile = profile.await()
    )
}
```

### SupervisorScope

Use `supervisorScope` when child failures should not cancel siblings:

```kotlin
suspend fun syncAll() = supervisorScope {
    launch { syncItems() }       // failure here won't cancel syncStats
    launch { syncStats() }
    launch { syncSettings() }
}
```

## Flow Patterns

### Cold Flow — One-Shot to Stream Conversion

```kotlin
fun observeItems(): Flow<List<Item>> = flow {
    // Re-emits whenever the database changes
    itemDao.observeAll()
        .map { entities -> entities.map { it.toDomain() } }
        .collect { emit(it) }
}
```

### StateFlow for UI State

```kotlin
class DashboardViewModel(
    observeProgress: ObserveUserProgressUseCase
) : ViewModel() {
    val progress: StateFlow<UserProgress> = observeProgress()
        .stateIn(
            scope = viewModelScope,
            started = SharingStarted.WhileSubscribed(5_000),
            initialValue = UserProgress.EMPTY
        )
}
```

`WhileSubscribed(5_000)` keeps the upstream active for 5 seconds after the last subscriber leaves — survives configuration changes without restarting.

### Combining Multiple Flows

```kotlin
val uiState: StateFlow<HomeState> = combine(
    itemRepository.observeItems(),
    settingsRepository.observeTheme(),
    userRepository.observeProfile()
) { items, theme, profile ->
    HomeState(items = items, theme = theme, profile = profile)
}.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())
```

### Flow Operators

```kotlin
// Debounce search input
searchQuery
    .debounce(300)
    .distinctUntilChanged()
    .flatMapLatest { query -> repository.search(query) }
    .catch { emit(emptyList()) }
    .collect { results -> _state.update { it.copy(results = results) } }

// Retry with exponential backoff
fun fetchWithRetry(): Flow<Data> = flow { emit(api.fetch()) }
    .retryWhen { cause, attempt ->
        if (cause is IOException && attempt < 3) {
            delay(1000L * (1 shl attempt.toInt()))
            true
        } else {
            false
        }
    }
```

### SharedFlow for One-Time Events

```kotlin
class ItemListViewModel : ViewModel() {
    private val _effects = MutableSharedFlow<Effect>()
    val effects: SharedFlow<Effect> = _effects.asSharedFlow()

    sealed interface Effect {
        data class ShowSnackbar(val message: String) : Effect
        data class NavigateTo(val route: String) : Effect
    }

    private fun deleteItem(id: String) {
        viewModelScope.launch {
            repository.delete(id)
            _effects.emit(Effect.ShowSnackbar("Item deleted"))
        }
    }
}

// Collect in Composable
LaunchedEffect(Unit) {
    viewModel.effects.collect { effect ->
        when (effect) {
            is Effect.ShowSnackbar -> snackbarHostState.showSnackbar(effect.message)
            is Effect.NavigateTo -> navController.navigate(effect.route)
        }
    }
}
```

## Dispatchers

```kotlin
// CPU-intensive work
withContext(Dispatchers.Default) { parseJson(largePayload) }

// IO-bound work
withContext(Dispatchers.IO) { database.query() }

// Main thread (UI) — default in viewModelScope
withContext(Dispatchers.Main) { updateUi() }
```

In KMP, use `Dispatchers.Default` and `Dispatchers.Main` (available on all platforms). `Dispatchers.IO` is JVM/Android only — use `Dispatchers.Default` on other platforms or provide via DI.

## Cancellation

### Cooperative Cancellation

Long-running loops must check for cancellation:

```kotlin
suspend fun processItems(items: List<Item>) = coroutineScope {
    for (item in items) {
        ensureActive()  // throws CancellationException if cancelled
        process(item)
    }
}
```

### Cleanup with try/finally

```kotlin
viewModelScope.launch {
    try {
        _state.update { it.copy(isLoading = true) }
        val data = repository.fetch()
        _state.update { it.copy(data = data) }
    } finally {
        _state.update { it.copy(isLoading = false) }  // always runs, even on cancellation
    }
}
```

## Testing

### Testing StateFlow with Turbine

```kotlin
@Test
fun `search updates item list`() = runTest {
    val fakeRepository = FakeItemRepository().apply { emit(testItems) }
    val viewModel = ItemListViewModel(GetItemsUseCase(fakeRepository))

    viewModel.state.test {
        assertEquals(ItemListState(), awaitItem())  // initial

        viewModel.onSearch("query")
        val loading = awaitItem()
        assertTrue(loading.isLoading)

        val loaded = awaitItem()
        assertFalse(loaded.isLoading)
        assertEquals(1, loaded.items.size)
    }
}
```

### Testing with TestDispatcher

```kotlin
@Test
fun `parallel load completes correctly`() = runTest {
    val viewModel = DashboardViewModel(
        itemRepo = FakeItemRepo(),
        statsRepo = FakeStatsRepo()
    )

    viewModel.load()
    advanceUntilIdle()

    val state = viewModel.state.value
    assertNotNull(state.items)
    assertNotNull(state.stats)
}
```

### Faking Flows

```kotlin
class FakeItemRepository : ItemRepository {
    private val _items = MutableStateFlow<List<Item>>(emptyList())

    override fun observeItems(): Flow<List<Item>> = _items

    fun emit(items: List<Item>) { _items.value = items }

    override suspend fun getItemsByCategory(category: String): Result<List<Item>> {
        return Result.success(_items.value.filter { it.category == category })
    }
}
```

## Anti-Patterns to Avoid

- Using `GlobalScope` — leaks coroutines, no structured cancellation
- Collecting Flows in `init {}` without a scope — use `viewModelScope.launch`
- Using `MutableStateFlow` with mutable collections — always use immutable copies: `_state.update { it.copy(list = it.list + newItem) }`
- Catching `CancellationException` — let it propagate for proper cancellation
- Using `flowOn(Dispatchers.Main)` to collect — collection dispatcher is the caller's dispatcher
- Creating `Flow` in `@Composable` without `remember` — recreates the flow every recomposition

## References

See skill: `compose-multiplatform-patterns` for UI consumption of Flows.
See skill: `android-clean-architecture` for where coroutines fit in layers.


---

---
name: kotlin-exposed-patterns
description: JetBrains Exposed ORM patterns including DSL queries, DAO pattern, transactions, HikariCP connection pooling, Flyway migrations, and repository pattern.
origin: ECC
---

# Kotlin Exposed Patterns

Comprehensive patterns for database access with JetBrains Exposed ORM, including DSL queries, DAO, transactions, and production-ready configuration.

## When to Use

- Setting up database access with Exposed
- Writing SQL queries using Exposed DSL or DAO
- Configuring connection pooling with HikariCP
- Creating database migrations with Flyway
- Implementing the repository pattern with Exposed
- Handling JSON columns and complex queries

## How It Works

Exposed provides two query styles: DSL for direct SQL-like expressions and DAO for entity lifecycle management. HikariCP manages a pool of reusable database connections configured via `HikariConfig`. Flyway runs versioned SQL migration scripts at startup to keep the schema in sync. All database operations run inside `newSuspendedTransaction` blocks for coroutine safety and atomicity. The repository pattern wraps Exposed queries behind an interface so business logic stays decoupled from the data layer and tests can use an in-memory H2 database.

## Examples

### DSL Query

```kotlin
suspend fun findUserById(id: UUID): UserRow? =
    newSuspendedTransaction {
        UsersTable.selectAll()
            .where { UsersTable.id eq id }
            .map { it.toUser() }
            .singleOrNull()
    }
```

### DAO Entity Usage

```kotlin
suspend fun createUser(request: CreateUserRequest): User =
    newSuspendedTransaction {
        UserEntity.new {
            name = request.name
            email = request.email
            role = request.role
        }.toModel()
    }
```

### HikariCP Configuration

```kotlin
val hikariConfig = HikariConfig().apply {
    driverClassName = config.driver
    jdbcUrl = config.url
    username = config.username
    password = config.password
    maximumPoolSize = config.maxPoolSize
    isAutoCommit = false
    transactionIsolation = "TRANSACTION_READ_COMMITTED"
    validate()
}
```

## Database Setup

### HikariCP Connection Pooling

```kotlin
// DatabaseFactory.kt
object DatabaseFactory {
    fun create(config: DatabaseConfig): Database {
        val hikariConfig = HikariConfig().apply {
            driverClassName = config.driver
            jdbcUrl = config.url
            username = config.username
            password = config.password
            maximumPoolSize = config.maxPoolSize
            isAutoCommit = false
            transactionIsolation = "TRANSACTION_READ_COMMITTED"
            validate()
        }

        return Database.connect(HikariDataSource(hikariConfig))
    }
}

data class DatabaseConfig(
    val url: String,
    val driver: String = "org.postgresql.Driver",
    val username: String = "",
    val password: String = "",
    val maxPoolSize: Int = 10,
)
```

### Flyway Migrations

```kotlin
// FlywayMigration.kt
fun runMigrations(config: DatabaseConfig) {
    Flyway.configure()
        .dataSource(config.url, config.username, config.password)
        .locations("classpath:db/migration")
        .baselineOnMigrate(true)
        .load()
        .migrate()
}

// Application startup
fun Application.module() {
    val config = DatabaseConfig(
        url = environment.config.property("database.url").getString(),
        username = environment.config.property("database.username").getString(),
        password = environment.config.property("database.password").getString(),
    )
    runMigrations(config)
    val database = DatabaseFactory.create(config)
    // ...
}
```

### Migration Files

```sql
-- src/main/resources/db/migration/V1__create_users.sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    role VARCHAR(20) NOT NULL DEFAULT 'USER',
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);
```

## Table Definitions

### DSL Style Tables

```kotlin
// tables/UsersTable.kt
object UsersTable : UUIDTable("users") {
    val name = varchar("name", 100)
    val email = varchar("email", 255).uniqueIndex()
    val role = enumerationByName<Role>("role", 20)
    val metadata = jsonb<UserMetadata>("metadata", Json.Default).nullable()
    val createdAt = timestampWithTimeZone("created_at").defaultExpression(CurrentTimestampWithTimeZone)
    val updatedAt = timestampWithTimeZone("updated_at").defaultExpression(CurrentTimestampWithTimeZone)
}

object OrdersTable : UUIDTable("orders") {
    val userId = uuid("user_id").references(UsersTable.id)
    val status = enumerationByName<OrderStatus>("status", 20)
    val totalAmount = long("total_amount")
    val currency = varchar("currency", 3)
    val createdAt = timestampWithTimeZone("created_at").defaultExpression(CurrentTimestampWithTimeZone)
}

object OrderItemsTable : UUIDTable("order_items") {
    val orderId = uuid("order_id").references(OrdersTable.id, onDelete = ReferenceOption.CASCADE)
    val productId = uuid("product_id")
    val quantity = integer("quantity")
    val unitPrice = long("unit_price")
}
```

### Composite Tables

```kotlin
object UserRolesTable : Table("user_roles") {
    val userId = uuid("user_id").references(UsersTable.id, onDelete = ReferenceOption.CASCADE)
    val roleId = uuid("role_id").references(RolesTable.id, onDelete = ReferenceOption.CASCADE)
    override val primaryKey = PrimaryKey(userId, roleId)
}
```

## DSL Queries

### Basic CRUD

```kotlin
// Insert
suspend fun insertUser(name: String, email: String, role: Role): UUID =
    newSuspendedTransaction {
        UsersTable.insertAndGetId {
            it[UsersTable.name] = name
            it[UsersTable.email] = email
            it[UsersTable.role] = role
        }.value
    }

// Select by ID
suspend fun findUserById(id: UUID): UserRow? =
    newSuspendedTransaction {
        UsersTable.selectAll()
            .where { UsersTable.id eq id }
            .map { it.toUser() }
            .singleOrNull()
    }

// Select with conditions
suspend fun findActiveAdmins(): List<UserRow> =
    newSuspendedTransaction {
        UsersTable.selectAll()
            .where { (UsersTable.role eq Role.ADMIN) }
            .orderBy(UsersTable.name)
            .map { it.toUser() }
    }

// Update
suspend fun updateUserEmail(id: UUID, newEmail: String): Boolean =
    newSuspendedTransaction {
        UsersTable.update({ UsersTable.id eq id }) {
            it[email] = newEmail
            it[updatedAt] = CurrentTimestampWithTimeZone
        } > 0
    }

// Delete
suspend fun deleteUser(id: UUID): Boolean =
    newSuspendedTransaction {
        UsersTable.deleteWhere { UsersTable.id eq id } > 0
    }

// Row mapping
private fun ResultRow.toUser() = UserRow(
    id = this[UsersTable.id].value,
    name = this[UsersTable.name],
    email = this[UsersTable.email],
    role = this[UsersTable.role],
    metadata = this[UsersTable.metadata],
    createdAt = this[UsersTable.createdAt],
    updatedAt = this[UsersTable.updatedAt],
)
```

### Advanced Queries

```kotlin
// Join queries
suspend fun findOrdersWithUser(userId: UUID): List<OrderWithUser> =
    newSuspendedTransaction {
        (OrdersTable innerJoin UsersTable)
            .selectAll()
            .where { OrdersTable.userId eq userId }
            .orderBy(OrdersTable.createdAt, SortOrder.DESC)
            .map { row ->
                OrderWithUser(
                    orderId = row[OrdersTable.id].value,
                    status = row[OrdersTable.status],
                    totalAmount = row[OrdersTable.totalAmount],
                    userName = row[UsersTable.name],
                )
            }
    }

// Aggregation
suspend fun countUsersByRole(): Map<Role, Long> =
    newSuspendedTransaction {
        UsersTable
            .select(UsersTable.role, UsersTable.id.count())
            .groupBy(UsersTable.role)
            .associate { row ->
                row[UsersTable.role] to row[UsersTable.id.count()]
            }
    }

// Subqueries
suspend fun findUsersWithOrders(): List<UserRow> =
    newSuspendedTransaction {
        UsersTable.selectAll()
            .where {
                UsersTable.id inSubQuery
                    OrdersTable.select(OrdersTable.userId).withDistinct()
            }
            .map { it.toUser() }
    }

// LIKE and pattern matching — always escape user input to prevent wildcard injection
private fun escapeLikePattern(input: String): String =
    input.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")

suspend fun searchUsers(query: String): List<UserRow> =
    newSuspendedTransaction {
        val sanitized = escapeLikePattern(query.lowercase())
        UsersTable.selectAll()
            .where {
                (UsersTable.name.lowerCase() like "%${sanitized}%") or
                    (UsersTable.email.lowerCase() like "%${sanitized}%")
            }
            .map { it.toUser() }
    }
```

### Pagination

```kotlin
data class Page<T>(
    val data: List<T>,
    val total: Long,
    val page: Int,
    val limit: Int,
) {
    val totalPages: Int get() = ((total + limit - 1) / limit).toInt()
    val hasNext: Boolean get() = page < totalPages
    val hasPrevious: Boolean get() = page > 1
}

suspend fun findUsersPaginated(page: Int, limit: Int): Page<UserRow> =
    newSuspendedTransaction {
        val total = UsersTable.selectAll().count()
        val data = UsersTable.selectAll()
            .orderBy(UsersTable.createdAt, SortOrder.DESC)
            .limit(limit)
            .offset(((page - 1) * limit).toLong())
            .map { it.toUser() }

        Page(data = data, total = total, page = page, limit = limit)
    }
```

### Batch Operations

```kotlin
// Batch insert
suspend fun insertUsers(users: List<CreateUserRequest>): List<UUID> =
    newSuspendedTransaction {
        UsersTable.batchInsert(users) { user ->
            this[UsersTable.name] = user.name
            this[UsersTable.email] = user.email
            this[UsersTable.role] = user.role
        }.map { it[UsersTable.id].value }
    }

// Upsert (insert or update on conflict)
suspend fun upsertUser(id: UUID, name: String, email: String) {
    newSuspendedTransaction {
        UsersTable.upsert(UsersTable.email) {
            it[UsersTable.id] = EntityID(id, UsersTable)
            it[UsersTable.name] = name
            it[UsersTable.email] = email
            it[updatedAt] = CurrentTimestampWithTimeZone
        }
    }
}
```

## DAO Pattern

### Entity Definitions

```kotlin
// entities/UserEntity.kt
class UserEntity(id: EntityID<UUID>) : UUIDEntity(id) {
    companion object : UUIDEntityClass<UserEntity>(UsersTable)

    var name by UsersTable.name
    var email by UsersTable.email
    var role by UsersTable.role
    var metadata by UsersTable.metadata
    var createdAt by UsersTable.createdAt
    var updatedAt by UsersTable.updatedAt

    val orders by OrderEntity referrersOn OrdersTable.userId

    fun toModel(): User = User(
        id = id.value,
        name = name,
        email = email,
        role = role,
        metadata = metadata,
        createdAt = createdAt,
        updatedAt = updatedAt,
    )
}

class OrderEntity(id: EntityID<UUID>) : UUIDEntity(id) {
    companion object : UUIDEntityClass<OrderEntity>(OrdersTable)

    var user by UserEntity referencedOn OrdersTable.userId
    var status by OrdersTable.status
    var totalAmount by OrdersTable.totalAmount
    var currency by OrdersTable.currency
    var createdAt by OrdersTable.createdAt

    val items by OrderItemEntity referrersOn OrderItemsTable.orderId
}
```

### DAO Operations

```kotlin
suspend fun findUserByEmail(email: String): User? =
    newSuspendedTransaction {
        UserEntity.find { UsersTable.email eq email }
            .firstOrNull()
            ?.toModel()
    }

suspend fun createUser(request: CreateUserRequest): User =
    newSuspendedTransaction {
        UserEntity.new {
            name = request.name
            email = request.email
            role = request.role
        }.toModel()
    }

suspend fun updateUser(id: UUID, request: UpdateUserRequest): User? =
    newSuspendedTransaction {
        UserEntity.findById(id)?.apply {
            request.name?.let { name = it }
            request.email?.let { email = it }
            updatedAt = OffsetDateTime.now(ZoneOffset.UTC)
        }?.toModel()
    }
```

## Transactions

### Suspend Transaction Support

```kotlin
// Good: Use newSuspendedTransaction for coroutine support
suspend fun performDatabaseOperation(): Result<User> =
    runCatching {
        newSuspendedTransaction {
            val user = UserEntity.new {
                name = "Alice"
                email = "alice@example.com"
            }
            // All operations in this block are atomic
            user.toModel()
        }
    }

// Good: Nested transactions with savepoints
suspend fun transferFunds(fromId: UUID, toId: UUID, amount: Long) {
    newSuspendedTransaction {
        val from = UserEntity.findById(fromId) ?: throw NotFoundException("User $fromId not found")
        val to = UserEntity.findById(toId) ?: throw NotFoundException("User $toId not found")

        // Debit
        from.balance -= amount
        // Credit
        to.balance += amount

        // Both succeed or both fail
    }
}
```

### Transaction Isolation

```kotlin
suspend fun readCommittedQuery(): List<User> =
    newSuspendedTransaction(transactionIsolation = Connection.TRANSACTION_READ_COMMITTED) {
        UserEntity.all().map { it.toModel() }
    }

suspend fun serializableOperation() {
    newSuspendedTransaction(transactionIsolation = Connection.TRANSACTION_SERIALIZABLE) {
        // Strictest isolation level for critical operations
    }
}
```

## Repository Pattern

### Interface Definition

```kotlin
interface UserRepository {
    suspend fun findById(id: UUID): User?
    suspend fun findByEmail(email: String): User?
    suspend fun findAll(page: Int, limit: Int): Page<User>
    suspend fun search(query: String): List<User>
    suspend fun create(request: CreateUserRequest): User
    suspend fun update(id: UUID, request: UpdateUserRequest): User?
    suspend fun delete(id: UUID): Boolean
    suspend fun count(): Long
}
```

### Exposed Implementation

```kotlin
class ExposedUserRepository(
    private val database: Database,
) : UserRepository {

    override suspend fun findById(id: UUID): User? =
        newSuspendedTransaction(db = database) {
            UsersTable.selectAll()
                .where { UsersTable.id eq id }
                .map { it.toUser() }
                .singleOrNull()
        }

    override suspend fun findByEmail(email: String): User? =
        newSuspendedTransaction(db = database) {
            UsersTable.selectAll()
                .where { UsersTable.email eq email }
                .map { it.toUser() }
                .singleOrNull()
        }

    override suspend fun findAll(page: Int, limit: Int): Page<User> =
        newSuspendedTransaction(db = database) {
            val total = UsersTable.selectAll().count()
            val data = UsersTable.selectAll()
                .orderBy(UsersTable.createdAt, SortOrder.DESC)
                .limit(limit)
                .offset(((page - 1) * limit).toLong())
                .map { it.toUser() }
            Page(data = data, total = total, page = page, limit = limit)
        }

    override suspend fun search(query: String): List<User> =
        newSuspendedTransaction(db = database) {
            val sanitized = escapeLikePattern(query.lowercase())
            UsersTable.selectAll()
                .where {
                    (UsersTable.name.lowerCase() like "%${sanitized}%") or
                        (UsersTable.email.lowerCase() like "%${sanitized}%")
                }
                .orderBy(UsersTable.name)
                .map { it.toUser() }
        }

    override suspend fun create(request: CreateUserRequest): User =
        newSuspendedTransaction(db = database) {
            UsersTable.insert {
                it[name] = request.name
                it[email] = request.email
                it[role] = request.role
            }.resultedValues!!.first().toUser()
        }

    override suspend fun update(id: UUID, request: UpdateUserRequest): User? =
        newSuspendedTransaction(db = database) {
            val updated = UsersTable.update({ UsersTable.id eq id }) {
                request.name?.let { name -> it[UsersTable.name] = name }
                request.email?.let { email -> it[UsersTable.email] = email }
                it[updatedAt] = CurrentTimestampWithTimeZone
            }
            if (updated > 0) findById(id) else null
        }

    override suspend fun delete(id: UUID): Boolean =
        newSuspendedTransaction(db = database) {
            UsersTable.deleteWhere { UsersTable.id eq id } > 0
        }

    override suspend fun count(): Long =
        newSuspendedTransaction(db = database) {
            UsersTable.selectAll().count()
        }

    private fun ResultRow.toUser() = User(
        id = this[UsersTable.id].value,
        name = this[UsersTable.name],
        email = this[UsersTable.email],
        role = this[UsersTable.role],
        metadata = this[UsersTable.metadata],
        createdAt = this[UsersTable.createdAt],
        updatedAt = this[UsersTable.updatedAt],
    )
}
```

## JSON Columns

### JSONB with kotlinx.serialization

```kotlin
// Custom column type for JSONB
inline fun <reified T : Any> Table.jsonb(
    name: String,
    json: Json,
): Column<T> = registerColumn(name, object : ColumnType<T>() {
    override fun sqlType() = "JSONB"

    override fun valueFromDB(value: Any): T = when (value) {
        is String -> json.decodeFromString(value)
        is PGobject -> {
            val jsonString = value.value
                ?: throw IllegalArgumentException("PGobject value is null for column '$name'")
            json.decodeFromString(jsonString)
        }
        else -> throw IllegalArgumentException("Unexpected value: $value")
    }

    override fun notNullValueToDB(value: T): Any =
        PGobject().apply {
            type = "jsonb"
            this.value = json.encodeToString(value)
        }
})

// Usage in table
@Serializable
data class UserMetadata(
    val preferences: Map<String, String> = emptyMap(),
    val tags: List<String> = emptyList(),
)

object UsersTable : UUIDTable("users") {
    val metadata = jsonb<UserMetadata>("metadata", Json.Default).nullable()
}
```

## Testing with Exposed

### In-Memory Database for Tests

```kotlin
class UserRepositoryTest : FunSpec({
    lateinit var database: Database
    lateinit var repository: UserRepository

    beforeSpec {
        database = Database.connect(
            url = "jdbc:h2:mem:test;DB_CLOSE_DELAY=-1;MODE=PostgreSQL",
            driver = "org.h2.Driver",
        )
        transaction(database) {
            SchemaUtils.create(UsersTable)
        }
        repository = ExposedUserRepository(database)
    }

    beforeTest {
        transaction(database) {
            UsersTable.deleteAll()
        }
    }

    test("create and find user") {
        val user = repository.create(CreateUserRequest("Alice", "alice@example.com"))

        user.name shouldBe "Alice"
        user.email shouldBe "alice@example.com"

        val found = repository.findById(user.id)
        found shouldBe user
    }

    test("findByEmail returns null for unknown email") {
        val result = repository.findByEmail("unknown@example.com")
        result.shouldBeNull()
    }

    test("pagination works correctly") {
        repeat(25) { i ->
            repository.create(CreateUserRequest("User $i", "user$i@example.com"))
        }

        val page1 = repository.findAll(page = 1, limit = 10)
        page1.data shouldHaveSize 10
        page1.total shouldBe 25
        page1.hasNext shouldBe true

        val page3 = repository.findAll(page = 3, limit = 10)
        page3.data shouldHaveSize 5
        page3.hasNext shouldBe false
    }
})
```

## Gradle Dependencies

```kotlin
// build.gradle.kts
dependencies {
    // Exposed
    implementation("org.jetbrains.exposed:exposed-core:1.0.0")
    implementation("org.jetbrains.exposed:exposed-dao:1.0.0")
    implementation("org.jetbrains.exposed:exposed-jdbc:1.0.0")
    implementation("org.jetbrains.exposed:exposed-kotlin-datetime:1.0.0")
    implementation("org.jetbrains.exposed:exposed-json:1.0.0")

    // Database driver
    implementation("org.postgresql:postgresql:42.7.5")

    // Connection pooling
    implementation("com.zaxxer:HikariCP:6.2.1")

    // Migrations
    implementation("org.flywaydb:flyway-core:10.22.0")
    implementation("org.flywaydb:flyway-database-postgresql:10.22.0")

    // Testing
    testImplementation("com.h2database:h2:2.3.232")
}
```

## Quick Reference: Exposed Patterns

| Pattern | Description |
|---------|-------------|
| `object Table : UUIDTable("name")` | Define table with UUID primary key |
| `newSuspendedTransaction { }` | Coroutine-safe transaction block |
| `Table.selectAll().where { }` | Query with conditions |
| `Table.insertAndGetId { }` | Insert and return generated ID |
| `Table.update({ condition }) { }` | Update matching rows |
| `Table.deleteWhere { }` | Delete matching rows |
| `Table.batchInsert(items) { }` | Efficient bulk insert |
| `innerJoin` / `leftJoin` | Join tables |
| `orderBy` / `limit` / `offset` | Sort and paginate |
| `count()` / `sum()` / `avg()` | Aggregation functions |

**Remember**: Use the DSL style for simple queries and the DAO style when you need entity lifecycle management. Always use `newSuspendedTransaction` for coroutine support, and wrap database operations behind a repository interface for testability.


---

---
name: kotlin-ktor-patterns
description: Ktor server patterns including routing DSL, plugins, authentication, Koin DI, kotlinx.serialization, WebSockets, and testApplication testing.
origin: ECC
---

# Ktor Server Patterns

Comprehensive Ktor patterns for building robust, maintainable HTTP servers with Kotlin coroutines.

## When to Activate

- Building Ktor HTTP servers
- Configuring Ktor plugins (Auth, CORS, ContentNegotiation, StatusPages)
- Implementing REST APIs with Ktor
- Setting up dependency injection with Koin
- Writing Ktor integration tests with testApplication
- Working with WebSockets in Ktor

## Application Structure

### Standard Ktor Project Layout

```text
src/main/kotlin/
├── com/example/
│   ├── Application.kt           # Entry point, module configuration
│   ├── plugins/
│   │   ├── Routing.kt           # Route definitions
│   │   ├── Serialization.kt     # Content negotiation setup
│   │   ├── Authentication.kt    # Auth configuration
│   │   ├── StatusPages.kt       # Error handling
│   │   └── CORS.kt              # CORS configuration
│   ├── routes/
│   │   ├── UserRoutes.kt        # /users endpoints
│   │   ├── AuthRoutes.kt        # /auth endpoints
│   │   └── HealthRoutes.kt      # /health endpoints
│   ├── models/
│   │   ├── User.kt              # Domain models
│   │   └── ApiResponse.kt       # Response envelopes
│   ├── services/
│   │   ├── UserService.kt       # Business logic
│   │   └── AuthService.kt       # Auth logic
│   ├── repositories/
│   │   ├── UserRepository.kt    # Data access interface
│   │   └── ExposedUserRepository.kt
│   └── di/
│       └── AppModule.kt         # Koin modules
src/test/kotlin/
├── com/example/
│   ├── routes/
│   │   └── UserRoutesTest.kt
│   └── services/
│       └── UserServiceTest.kt
```

### Application Entry Point

```kotlin
// Application.kt
fun main() {
    embeddedServer(Netty, port = 8080, module = Application::module).start(wait = true)
}

fun Application.module() {
    configureSerialization()
    configureAuthentication()
    configureStatusPages()
    configureCORS()
    configureDI()
    configureRouting()
}
```

## Routing DSL

### Basic Routes

```kotlin
// plugins/Routing.kt
fun Application.configureRouting() {
    routing {
        userRoutes()
        authRoutes()
        healthRoutes()
    }
}

// routes/UserRoutes.kt
fun Route.userRoutes() {
    val userService by inject<UserService>()

    route("/users") {
        get {
            val users = userService.getAll()
            call.respond(users)
        }

        get("/{id}") {
            val id = call.parameters["id"]
                ?: return@get call.respond(HttpStatusCode.BadRequest, "Missing id")
            val user = userService.getById(id)
                ?: return@get call.respond(HttpStatusCode.NotFound)
            call.respond(user)
        }

        post {
            val request = call.receive<CreateUserRequest>()
            val user = userService.create(request)
            call.respond(HttpStatusCode.Created, user)
        }

        put("/{id}") {
            val id = call.parameters["id"]
                ?: return@put call.respond(HttpStatusCode.BadRequest, "Missing id")
            val request = call.receive<UpdateUserRequest>()
            val user = userService.update(id, request)
                ?: return@put call.respond(HttpStatusCode.NotFound)
            call.respond(user)
        }

        delete("/{id}") {
            val id = call.parameters["id"]
                ?: return@delete call.respond(HttpStatusCode.BadRequest, "Missing id")
            val deleted = userService.delete(id)
            if (deleted) call.respond(HttpStatusCode.NoContent)
            else call.respond(HttpStatusCode.NotFound)
        }
    }
}
```

### Route Organization with Authenticated Routes

```kotlin
fun Route.userRoutes() {
    route("/users") {
        // Public routes
        get { /* list users */ }
        get("/{id}") { /* get user */ }

        // Protected routes
        authenticate("jwt") {
            post { /* create user - requires auth */ }
            put("/{id}") { /* update user - requires auth */ }
            delete("/{id}") { /* delete user - requires auth */ }
        }
    }
}
```

## Content Negotiation & Serialization

### kotlinx.serialization Setup

```kotlin
// plugins/Serialization.kt
fun Application.configureSerialization() {
    install(ContentNegotiation) {
        json(Json {
            prettyPrint = true
            isLenient = false
            ignoreUnknownKeys = true
            encodeDefaults = true
            explicitNulls = false
        })
    }
}
```

### Serializable Models

```kotlin
@Serializable
data class UserResponse(
    val id: String,
    val name: String,
    val email: String,
    val role: Role,
    @Serializable(with = InstantSerializer::class)
    val createdAt: Instant,
)

@Serializable
data class CreateUserRequest(
    val name: String,
    val email: String,
    val role: Role = Role.USER,
)

@Serializable
data class ApiResponse<T>(
    val success: Boolean,
    val data: T? = null,
    val error: String? = null,
) {
    companion object {
        fun <T> ok(data: T): ApiResponse<T> = ApiResponse(success = true, data = data)
        fun <T> error(message: String): ApiResponse<T> = ApiResponse(success = false, error = message)
    }
}

@Serializable
data class PaginatedResponse<T>(
    val data: List<T>,
    val total: Long,
    val page: Int,
    val limit: Int,
)
```

### Custom Serializers

```kotlin
object InstantSerializer : KSerializer<Instant> {
    override val descriptor = PrimitiveSerialDescriptor("Instant", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: Instant) =
        encoder.encodeString(value.toString())
    override fun deserialize(decoder: Decoder): Instant =
        Instant.parse(decoder.decodeString())
}
```

## Authentication

### JWT Authentication

```kotlin
// plugins/Authentication.kt
fun Application.configureAuthentication() {
    val jwtSecret = environment.config.property("jwt.secret").getString()
    val jwtIssuer = environment.config.property("jwt.issuer").getString()
    val jwtAudience = environment.config.property("jwt.audience").getString()
    val jwtRealm = environment.config.property("jwt.realm").getString()

    install(Authentication) {
        jwt("jwt") {
            realm = jwtRealm
            verifier(
                JWT.require(Algorithm.HMAC256(jwtSecret))
                    .withAudience(jwtAudience)
                    .withIssuer(jwtIssuer)
                    .build()
            )
            validate { credential ->
                if (credential.payload.audience.contains(jwtAudience)) {
                    JWTPrincipal(credential.payload)
                } else {
                    null
                }
            }
            challenge { _, _ ->
                call.respond(HttpStatusCode.Unauthorized, ApiResponse.error<Unit>("Invalid or expired token"))
            }
        }
    }
}

// Extracting user from JWT
fun ApplicationCall.userId(): String =
    principal<JWTPrincipal>()
        ?.payload
        ?.getClaim("userId")
        ?.asString()
        ?: throw AuthenticationException("No userId in token")
```

### Auth Routes

```kotlin
fun Route.authRoutes() {
    val authService by inject<AuthService>()

    route("/auth") {
        post("/login") {
            val request = call.receive<LoginRequest>()
            val token = authService.login(request.email, request.password)
                ?: return@post call.respond(
                    HttpStatusCode.Unauthorized,
                    ApiResponse.error<Unit>("Invalid credentials"),
                )
            call.respond(ApiResponse.ok(TokenResponse(token)))
        }

        post("/register") {
            val request = call.receive<RegisterRequest>()
            val user = authService.register(request)
            call.respond(HttpStatusCode.Created, ApiResponse.ok(user))
        }

        authenticate("jwt") {
            get("/me") {
                val userId = call.userId()
                val user = authService.getProfile(userId)
                call.respond(ApiResponse.ok(user))
            }
        }
    }
}
```

## Status Pages (Error Handling)

```kotlin
// plugins/StatusPages.kt
fun Application.configureStatusPages() {
    install(StatusPages) {
        exception<ContentTransformationException> { call, cause ->
            call.respond(
                HttpStatusCode.BadRequest,
                ApiResponse.error<Unit>("Invalid request body: ${cause.message}"),
            )
        }

        exception<IllegalArgumentException> { call, cause ->
            call.respond(
                HttpStatusCode.BadRequest,
                ApiResponse.error<Unit>(cause.message ?: "Bad request"),
            )
        }

        exception<AuthenticationException> { call, _ ->
            call.respond(
                HttpStatusCode.Unauthorized,
                ApiResponse.error<Unit>("Authentication required"),
            )
        }

        exception<AuthorizationException> { call, _ ->
            call.respond(
                HttpStatusCode.Forbidden,
                ApiResponse.error<Unit>("Access denied"),
            )
        }

        exception<NotFoundException> { call, cause ->
            call.respond(
                HttpStatusCode.NotFound,
                ApiResponse.error<Unit>(cause.message ?: "Resource not found"),
            )
        }

        exception<Throwable> { call, cause ->
            call.application.log.error("Unhandled exception", cause)
            call.respond(
                HttpStatusCode.InternalServerError,
                ApiResponse.error<Unit>("Internal server error"),
            )
        }

        status(HttpStatusCode.NotFound) { call, status ->
            call.respond(status, ApiResponse.error<Unit>("Route not found"))
        }
    }
}
```

## CORS Configuration

```kotlin
// plugins/CORS.kt
fun Application.configureCORS() {
    install(CORS) {
        allowHost("localhost:3000")
        allowHost("example.com", schemes = listOf("https"))
        allowHeader(HttpHeaders.ContentType)
        allowHeader(HttpHeaders.Authorization)
        allowMethod(HttpMethod.Put)
        allowMethod(HttpMethod.Delete)
        allowMethod(HttpMethod.Patch)
        allowCredentials = true
        maxAgeInSeconds = 3600
    }
}
```

## Koin Dependency Injection

### Module Definition

```kotlin
// di/AppModule.kt
val appModule = module {
    // Database
    single<Database> { DatabaseFactory.create(get()) }

    // Repositories
    single<UserRepository> { ExposedUserRepository(get()) }
    single<OrderRepository> { ExposedOrderRepository(get()) }

    // Services
    single { UserService(get()) }
    single { OrderService(get(), get()) }
    single { AuthService(get(), get()) }
}

// Application setup
fun Application.configureDI() {
    install(Koin) {
        modules(appModule)
    }
}
```

### Using Koin in Routes

```kotlin
fun Route.userRoutes() {
    val userService by inject<UserService>()

    route("/users") {
        get {
            val users = userService.getAll()
            call.respond(ApiResponse.ok(users))
        }
    }
}
```

### Koin for Testing

```kotlin
class UserServiceTest : FunSpec(), KoinTest {
    override fun extensions() = listOf(KoinExtension(testModule))

    private val testModule = module {
        single<UserRepository> { mockk() }
        single { UserService(get()) }
    }

    private val repository by inject<UserRepository>()
    private val service by inject<UserService>()

    init {
        test("getUser returns user") {
            coEvery { repository.findById("1") } returns testUser
            service.getById("1") shouldBe testUser
        }
    }
}
```

## Request Validation

```kotlin
// Validate request data in routes
fun Route.userRoutes() {
    val userService by inject<UserService>()

    post("/users") {
        val request = call.receive<CreateUserRequest>()

        // Validate
        require(request.name.isNotBlank()) { "Name is required" }
        require(request.name.length <= 100) { "Name must be 100 characters or less" }
        require(request.email.matches(Regex(".+@.+\\..+"))) { "Invalid email format" }

        val user = userService.create(request)
        call.respond(HttpStatusCode.Created, ApiResponse.ok(user))
    }
}

// Or use a validation extension
fun CreateUserRequest.validate() {
    require(name.isNotBlank()) { "Name is required" }
    require(name.length <= 100) { "Name must be 100 characters or less" }
    require(email.matches(Regex(".+@.+\\..+"))) { "Invalid email format" }
}
```

## WebSockets

```kotlin
fun Application.configureWebSockets() {
    install(WebSockets) {
        pingPeriod = 15.seconds
        timeout = 15.seconds
        maxFrameSize = 64 * 1024 // 64 KiB — increase only if your protocol requires larger frames
        masking = false // Server-to-client frames are unmasked per RFC 6455; client-to-server are always masked by Ktor
    }
}

fun Route.chatRoutes() {
    val connections = Collections.synchronizedSet<Connection>(LinkedHashSet())

    webSocket("/chat") {
        val thisConnection = Connection(this)
        connections += thisConnection

        try {
            send("Connected! Users online: ${connections.size}")

            for (frame in incoming) {
                frame as? Frame.Text ?: continue
                val text = frame.readText()
                val message = ChatMessage(thisConnection.name, text)

                // Snapshot under lock to avoid ConcurrentModificationException
                val snapshot = synchronized(connections) { connections.toList() }
                snapshot.forEach { conn ->
                    conn.session.send(Json.encodeToString(message))
                }
            }
        } catch (e: Exception) {
            logger.error("WebSocket error", e)
        } finally {
            connections -= thisConnection
        }
    }
}

data class Connection(val session: DefaultWebSocketSession) {
    val name: String = "User-${counter.getAndIncrement()}"

    companion object {
        private val counter = AtomicInteger(0)
    }
}
```

## testApplication Testing

### Basic Route Testing

```kotlin
class UserRoutesTest : FunSpec({
    test("GET /users returns list of users") {
        testApplication {
            application {
                install(Koin) { modules(testModule) }
                configureSerialization()
                configureRouting()
            }

            val response = client.get("/users")

            response.status shouldBe HttpStatusCode.OK
            val body = response.body<ApiResponse<List<UserResponse>>>()
            body.success shouldBe true
            body.data.shouldNotBeNull().shouldNotBeEmpty()
        }
    }

    test("POST /users creates a user") {
        testApplication {
            application {
                install(Koin) { modules(testModule) }
                configureSerialization()
                configureStatusPages()
                configureRouting()
            }

            val client = createClient {
                install(io.ktor.client.plugins.contentnegotiation.ContentNegotiation) {
                    json()
                }
            }

            val response = client.post("/users") {
                contentType(ContentType.Application.Json)
                setBody(CreateUserRequest("Alice", "alice@example.com"))
            }

            response.status shouldBe HttpStatusCode.Created
        }
    }

    test("GET /users/{id} returns 404 for unknown id") {
        testApplication {
            application {
                install(Koin) { modules(testModule) }
                configureSerialization()
                configureStatusPages()
                configureRouting()
            }

            val response = client.get("/users/unknown-id")

            response.status shouldBe HttpStatusCode.NotFound
        }
    }
})
```

### Testing Authenticated Routes

```kotlin
class AuthenticatedRoutesTest : FunSpec({
    test("protected route requires JWT") {
        testApplication {
            application {
                install(Koin) { modules(testModule) }
                configureSerialization()
                configureAuthentication()
                configureRouting()
            }

            val response = client.post("/users") {
                contentType(ContentType.Application.Json)
                setBody(CreateUserRequest("Alice", "alice@example.com"))
            }

            response.status shouldBe HttpStatusCode.Unauthorized
        }
    }

    test("protected route succeeds with valid JWT") {
        testApplication {
            application {
                install(Koin) { modules(testModule) }
                configureSerialization()
                configureAuthentication()
                configureRouting()
            }

            val token = generateTestJWT(userId = "test-user")

            val client = createClient {
                install(io.ktor.client.plugins.contentnegotiation.ContentNegotiation) { json() }
            }

            val response = client.post("/users") {
                contentType(ContentType.Application.Json)
                bearerAuth(token)
                setBody(CreateUserRequest("Alice", "alice@example.com"))
            }

            response.status shouldBe HttpStatusCode.Created
        }
    }
})
```

## Configuration

### application.yaml

```yaml
ktor:
  application:
    modules:
      - com.example.ApplicationKt.module
  deployment:
    port: 8080

jwt:
  secret: ${JWT_SECRET}
  issuer: "https://example.com"
  audience: "https://example.com/api"
  realm: "example"

database:
  url: ${DATABASE_URL}
  driver: "org.postgresql.Driver"
  maxPoolSize: 10
```

### Reading Config

```kotlin
fun Application.configureDI() {
    val dbUrl = environment.config.property("database.url").getString()
    val dbDriver = environment.config.property("database.driver").getString()
    val maxPoolSize = environment.config.property("database.maxPoolSize").getString().toInt()

    install(Koin) {
        modules(module {
            single { DatabaseConfig(dbUrl, dbDriver, maxPoolSize) }
            single { DatabaseFactory.create(get()) }
        })
    }
}
```

## Quick Reference: Ktor Patterns

| Pattern | Description |
|---------|-------------|
| `route("/path") { get { } }` | Route grouping with DSL |
| `call.receive<T>()` | Deserialize request body |
| `call.respond(status, body)` | Send response with status |
| `call.parameters["id"]` | Read path parameters |
| `call.request.queryParameters["q"]` | Read query parameters |
| `install(Plugin) { }` | Install and configure plugin |
| `authenticate("name") { }` | Protect routes with auth |
| `by inject<T>()` | Koin dependency injection |
| `testApplication { }` | Integration testing |

**Remember**: Ktor is designed around Kotlin coroutines and DSLs. Keep routes thin, push logic to services, and use Koin for dependency injection. Test with `testApplication` for full integration coverage.
