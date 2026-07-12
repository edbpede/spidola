---
type: "agent_requested"
description: "Kotlin 2.4 coding guidelines + Jetpack Compose for TV (androidx.tv) guidelines"
---
# Kotlin 2.4 & Jetpack Compose for TV — Combined Coding Reference

**Research date:** 11 July 2026 · **Research basis:** current official docs, release notes, specifications, changelogs, and primary repositories.

Kotlin 2.4.0 (released 3 June 2026) runs exclusively on the K2 compiler frontend — the K1 compiler was removed in this release (`-language-version=1.9` is no longer supported), so every project you touch compiles through K2 with `languageVersion`/`apiVersion` `2.4` as the floor. The compiler encodes this baseline directly: `FIRST_SUPPORTED = KOTLIN_2_0` and `LATEST_STABLE = KOTLIN_2_4`, meaning every 1.x language version is rejected outright. Kotlin targets the JVM (bytecode up to Java 26), Kotlin/Native (LLVM, Apple/Linux/Windows), Kotlin/Wasm, and Kotlin/JS, all from one language via Kotlin Multiplatform. Kotlin is exceptional at null-safe domain modelling with sealed hierarchies and exhaustive `when`, at structured-concurrency asynchronous code via coroutines and `Flow`, at reflection-free serialization, and at expressive type-safe DSLs. Optimize for immutability (`val`, read-only collection interfaces, `data class` with `copy`), for making illegal states unrepresentable in the type system, and for keeping side effects inside structured coroutine scopes.

On top of that language baseline, Compose for TV is now the default, production-grade way to build Android TV UIs: `androidx.tv:tv-material` is stable, its lazy-list story has been folded back into standard `androidx.compose.foundation`, and strong skipping is on by default. When targeting TV, optimize for one thing above all else: **D-pad focus correctness**. On a phone the finger is the cursor; on TV the *focus* is the cursor, and a screen that loses focus, traps focus, or restores it to the wrong place is broken regardless of how good it looks.

The dominant failure modes for an agent, in both halves of this document, are:

- **Java-in-Kotlin / old-Kotlin-in-new-Kotlin:** reaching for `!!` instead of `?:`/`requireNotNull`, using `java.util.stream` instead of Kotlin sequences, hand-rolling `synchronized` and thread pools instead of coroutines, launching on `GlobalScope`, calling `runBlocking` in production, mocking with Mockito instead of MockK, configuring the build with the removed `kotlinOptions {}` block instead of `compilerOptions {}`, or using the deprecated `context(...)` *context receivers* instead of the stable named *context parameters*.
- **Phone-Compose and legacy habits on TV:** reaching for `androidx.compose.material3` components (which have no TV focus indication) instead of `androidx.tv.material3`; using `Modifier.clickable` on a bare `Box` instead of a TV `Surface`/`Button` (so the item never shows a focus state); resurrecting `TvLazyRow`/`TvLazyColumn` (deleted) or the Leanback `androidx.leanback` fragments (legacy); and forgetting `focusRestorer()` so navigating back lands focus on item zero.

This document shows the one modern idiomatic way for each concern. Part I covers the Kotlin 2.4 language, stdlib, coroutines, and toolchain for any target; Part II covers Jetpack Compose for TV specifically. Assume the version floors annotated below are met and write the modern idiom once, correctly.

## Stack snapshot

### Core Kotlin (any target)

- **Language / compiler:** Kotlin 2.4.0, K2 only (K1 removed). `LATEST_STABLE = KOTLIN_2_4`; language versions below 2.0 are rejected. The JVM standard library now has an 18-month support window per release line.
- **Coroutines:** `org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0`
- **Serialization:** `org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0` + the `plugin.serialization` compiler plugin (versioned with Kotlin)
- **Build:** Gradle with the Kotlin DSL (`build.gradle.kts`), Kotlin Gradle Plugin 2.4.0, compatible with Gradle up to 9.5.0
- **Formatter/linter:** ktlint 1.8.0 or ktfmt 0.64 (formatting); detekt 1.23.8 stable (static analysis; detekt 2.0 is still alpha)
- **Testing:** `kotlin.test` + JUnit Jupiter 5.14.4, `kotlinx-coroutines-test` 1.11.0, MockK 1.14.3
- **KMP UI:** Compose Multiplatform 1.11.1 (iOS Stable, Web Beta)

### Android TV (Compose for TV)

| Component | Version | Notes |
|---|---|---|
| Kotlin | 2.4.0 | K2 only; K1 frontend removed. Context parameters stable. |
| Compose BOM | 2026.06.00 (core modules 1.11.x) | v2 test APIs default. |
| `androidx.tv:tv-material` | 1.1.0 | The TV Material 3 component library. |
| `androidx.tv:tv-foundation` | 1.0.0 | Only `TvImeOptions`/keyboard alignment remain; `TvLazy*` removed. |
| `androidx.compose.foundation` | 1.11.x | Source of `LazyRow`/`LazyColumn`/grids for TV. |
| Media3 (ExoPlayer) | 1.10.x | `media3-ui-compose` provides `PlayerSurface`. |
| Navigation 3 (`androidx.navigation3`) | 1.0.1 | Compose-first; back stack is your state. |
| Hilt (Dagger) | 2.57.1 | KSP annotation processing. |
| KSP | KSP2 (default) | KSP1 incompatible with Kotlin 2.3+/AGP 9. |
| Android Gradle Plugin | 8.13 / 9.0 | AGP 9 has built-in Kotlin, max API 36.1. |
| compileSdk / targetSdk | 36 | minSdk 23 typical for new TV apps. |

**Critical TV currency flags — reach for the right thing:**

- Use `androidx.tv.material3.*`, **not** `androidx.compose.material3.*`, for interactive TV components. The TV variants bake in focus scale/glow/border indication and D-pad semantics.
- `TvLazyRow`/`TvLazyColumn`/`TvLazyVerticalGrid` are **removed**. Use `LazyRow`/`LazyColumn`/`LazyVerticalGrid` from `androidx.compose.foundation.lazy`; TV pivot scrolling now comes from `LocalBringIntoViewSpec`.
- Leanback (`androidx.leanback`, `BrowseSupportFragment`, `Presenter`, `ArrayObjectAdapter`) is **legacy**. Do not use it for new apps.
- `ImmersiveList` was **removed** from tv-material. Build immersive/featured content yourself with `AnimatedContent` behind a `LazyRow` (shown in Part II).
- `Carousel` and all chips (`FilterChip`, `InputChip`, `AssistChip`, `SuggestionChip`) are still `@ExperimentalTvMaterial3Api`. Everything else you need — `Surface`, `Button`, `Card`, `TabRow`, `NavigationDrawer`, `ModalNavigationDrawer`, `ListItem`, `Checkbox`, `Switch`, `RadioButton` — is stable.

---

# Part I — Core Kotlin 2.4

## The K2 compiler and version settings

K2 is the only frontend in 2.4 — there is no flag to turn it off and nothing to opt into. Configure the language contract explicitly in the build so behaviour is reproducible, and turn on progressive mode to have the compiler apply the newest sound fixes for the current language version.

```kotlin
// build.gradle.kts
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinVersion

kotlin {
    compilerOptions {
        languageVersion.set(KotlinVersion.KOTLIN_2_4)
        apiVersion.set(KotlinVersion.KOTLIN_2_4)
        jvmTarget.set(JvmTarget.JVM_21)
        progressiveMode.set(true)          // apply newest sound deprecations for this language version
        allWarningsAsErrors.set(true)      // recommended for greenfield code
        freeCompilerArgs.add("-Xjsr305=strict") // treat JSR-305 nullability annotations as strict
    }
}
```

K2 smart-casting is more thorough than K1: it propagates casts across `&&`/`||` operands, through local `val` captured in the same scope, and out of `when` guards. You rarely need explicit casts. Exhaustiveness is enforced for `when` over sealed hierarchies and enums with no `else` branch, and an unhandled subtype is a compile error, not a warning.

## Null safety

Never use `!!` to silence the compiler. It is a runtime assertion that throws `NullPointerException`, and its presence in agent-written code is almost always a design error. (On TV, `!!` in a composable is a crash waiting for a slow network.) Choose an explicit strategy instead.

```kotlin
class UserService(private val repository: UserRepository) {

    fun greeting(userId: String): String {
        // Safe call + Elvis for a fallback value.
        val name: String = repository.findName(userId) ?: "guest"
        return "Hello, $name"
    }

    fun requireProfile(userId: String): Profile {
        // requireNotNull throws IllegalArgumentException with a message when the
        // invariant "this user must exist" is genuinely violated by the caller.
        val profile = repository.findProfile(userId)
        return requireNotNull(profile) { "No profile for user $userId" }
    }

    fun initials(userId: String): String? {
        // Safe-call chain returns null instead of throwing; map with let.
        return repository.findName(userId)
            ?.trim()
            ?.split(" ")
            ?.mapNotNull { it.firstOrNull()?.uppercaseChar() }
            ?.joinToString("")
    }
}
```

Use `lateinit var` only for non-null properties genuinely initialized after construction by a framework (dependency injection, `@BeforeEach` in tests) and never for primitives. Prefer `by lazy { }` when the value is computed once on first access. Reach for `Delegates.notNull()` for a non-null `var` of a primitive type set exactly once before reads.

```kotlin
import kotlin.properties.Delegates

class Config {
    val database: DatabaseClient by lazy { DatabaseClient.connect() } // computed once, thread-safe
    var maxConnections: Int by Delegates.notNull()                    // primitive, set once at startup
}
```

## Sealed hierarchies, `when`, and exhaustiveness

Model closed sets of states as `sealed interface`/`sealed class` and branch on them with an exhaustive `when` used as an *expression* (assign or return its result) so the compiler forces you to handle every case. Use guard conditions (stable since Kotlin 2.2) to add a secondary check to a branch without nesting.

```kotlin
sealed interface PaymentResult {
    data class Approved(val transactionId: String, val amount: Long) : PaymentResult
    data class Declined(val reason: String, val retryable: Boolean) : PaymentResult
    data object Pending : PaymentResult                 // data object: singleton with sensible toString (stable since 1.9)
}

fun describe(result: PaymentResult): String = when (result) {
    is PaymentResult.Approved                 -> "Charged ${result.amount} (tx ${result.transactionId})"
    is PaymentResult.Declined if result.retryable -> "Temporary decline: ${result.reason}, will retry"
    is PaymentResult.Declined                 -> "Declined permanently: ${result.reason}"
    PaymentResult.Pending                     -> "Awaiting confirmation"
    // No else branch: adding a new subtype makes this a compile error until handled.
}
```

Use `enum` for a fixed set of constants and iterate with the `entries` property (stable since 1.9), not the legacy `values()` which allocates a new array on every call.

```kotlin
enum class Weekday { MON, TUE, WED, THU, FRI }

val workdays: List<String> = Weekday.entries.map { it.name } // entries is a cached List, no allocation
```

## Data classes, destructuring, and value classes

`data class` gives you `equals`/`hashCode`/`toString`/`copy`/`componentN` for free. Keep them immutable (`val` properties) and evolve instances with `copy`.

```kotlin
data class Order(val id: String, val lines: List<Line>, val status: Status = Status.NEW) {
    data class Line(val sku: String, val quantity: Int)
    enum class Status { NEW, PAID, SHIPPED }
}

val paid = order.copy(status = Order.Status.PAID)
```

Destructure data classes and `Map.Entry` positionally, but be aware destructuring is positional — reordering `data class` properties silently rebinds names, so destructure only small, stable types. Name-based destructuring is still **experimental** (behind `-Xname-based-destructuring`) in 2.4 — do not use it in production.

```kotlin
val (id, lines) = order
for ((sku, qty) in lines.associate { it.sku to it.quantity }) { /* ... */ }
```

Wrap a single value in an inline value class (`@JvmInline`) to get a distinct type with no runtime allocation — the wrapper is erased to the underlying type at runtime. This prevents mixing up strings/ids of different meaning (e.g. a `MovieId` vs a raw `String` in a TV catalog).

```kotlin
@JvmInline
value class UserId(val value: String)

@JvmInline
value class Cents(val amount: Long) {
    init { require(amount >= 0) { "amount must be non-negative" } }
    operator fun plus(other: Cents) = Cents(amount + other.amount)
}

fun load(id: UserId): Profile = TODO() // callers cannot pass a raw String or an OrderId by mistake
```

## Scope functions

The five scope functions differ in what the lambda receives (`this` vs `it`) and what they return (the receiver vs the lambda result). Do not nest them — nested `let`/`apply` is a readability anti-pattern. Choose per this table.

| Function | Receives block as | Returns | Use for |
|----------|-------------------|---------|---------|
| `let`    | `it` (argument)   | lambda result | Transform a nullable with `?.let { }`; map a value to another |
| `run`    | `this` (receiver) | lambda result | Compute a result from an object's members; scope a block of statements |
| `with`   | `this` (receiver) | lambda result | Call several members of an already-non-null object |
| `apply`  | `this` (receiver) | the receiver  | Configure a mutable object then return it (builders, e.g. `ExoPlayer.Builder(...).build().apply { … }`) |
| `also`   | `it` (argument)   | the receiver  | Side effects that don't change the value (logging, validation) |

```kotlin
val request = HttpRequest().apply {          // configure-and-return -> apply
    method = "POST"
    header("Content-Type", "application/json")
}.also { logger.debug("built request {}", it) } // side effect, keep the value -> also

val length: Int? = nickname?.let { it.trim().length } // nullable transform -> let

val summary = with(order) {                  // several members of one object -> with
    "Order $id: ${lines.size} lines, $status"
}
```

## Functions

Prefer named and default arguments to overloads. Use single-expression bodies for functions that compute one value, and `vararg` with the spread operator `*` when forwarding.

```kotlin
fun connect(
    host: String,
    port: Int = 5432,
    ssl: Boolean = true,
    timeout: Duration = 30.seconds,
): Connection = Connection(host, port, ssl, timeout)

val c = connect("db.internal", ssl = false)          // skip defaults, name the ones you set

fun sumAll(vararg values: Int): Int = values.sum()
val extra = intArrayOf(4, 5, 6)
val total = sumAll(1, 2, 3, *extra)                   // spread into vararg
```

Infix functions read as operators for two-argument builders; `tailrec` turns self-recursion into a loop with no stack growth; local functions capture enclosing scope and keep helpers private to one function.

```kotlin
infix fun Int.upToBy(step: Int): IntProgression = this..100 step step

tailrec fun gcd(a: Long, b: Long): Long = if (b == 0L) a else gcd(b, a % b)
```

Mark higher-order functions `inline` so the lambda body is inlined at the call site with no `Function` allocation; use `reified` type parameters to access the type at runtime; use `crossinline` when the lambda must not do a non-local return and `noinline` to keep a specific lambda parameter un-inlined.

```kotlin
inline fun <reified T> Gson.fromJson(json: String): T =
    fromJson(json, T::class.java)                     // reified: T is available at runtime

inline fun <T> measured(label: String, block: () -> T): T {
    val start = System.nanoTime()
    try { return block() } finally { logger.info("$label took ${System.nanoTime() - start}ns") }
}
```

## Context parameters

Context parameters graduated to Stable in Kotlin 2.4.0 (except context arguments and callable references — per the release notes, "The following features have now graduated to Stable in Kotlin 2.4.0… Context parameters, except for context arguments and callable references"). They declare a dependency a function needs from its surrounding scope without threading it through every signature. Use them for genuine cross-cutting values that flow down a deep call graph — a logger, a clock, a transaction handle, a tenant id, an image loader. This is the modern replacement for the deprecated `context(...)` *context receivers*; the difference is that context parameters are named. Do not overuse them: a one-off dependency a single function needs is clearer as a plain parameter, and two context values of the same type in scope produce an ambiguity error.

```kotlin
interface Logger { fun info(message: String) }

// The function declares it needs a Logger available in context; callers don't pass it explicitly.
context(logger: Logger)
fun processOrder(order: Order) {
    logger.info("Processing order ${order.id}")
    // ... business logic, no logger parameter cluttering the signature
}

fun handle(order: Order, logger: Logger) {
    with(logger) {           // bring a Logger into context
        processOrder(order)  // resolved and passed automatically
    }
}
```

Note that *explicit context arguments* (naming the context value at the call site, e.g. `charge(log = primary)`) and *callable references* to context-parameter functions remain Experimental in 2.4 behind `-Xexplicit-context-arguments` (tracker KT-72222) — do not rely on them in production; pass through lambdas instead.

## Coroutines and structured concurrency

All asynchronous work runs inside a `CoroutineScope`; child coroutines are cancelled when their scope is cancelled. Never launch on `GlobalScope` (its coroutines outlive everything and leak) and never call `runBlocking` in production code (it blocks a thread, defeating the point of suspension) — `runBlocking` belongs in `main` entry points and tests only. Suspend functions should be main-safe: switch dispatchers internally with `withContext`, never force the caller to.

```kotlin
import kotlinx.coroutines.*

class ReportService(
    private val repo: ReportRepository,
    private val scope: CoroutineScope,   // injected, tied to a lifecycle
) {
    // Main-safe suspend function: caller need not know it does IO.
    suspend fun load(id: String): Report = withContext(Dispatchers.IO) {
        repo.fetch(id)
    }

    // Run independent fetches concurrently and fail fast if any fails.
    suspend fun loadDashboard(ids: List<String>): List<Report> = coroutineScope {
        ids.map { id -> async { load(id) } }.awaitAll()
    }
}
```

`launch` starts a fire-and-forget coroutine returning a `Job`; `async` starts one that returns a `Deferred<T>` you `await`. `coroutineScope` fails as a unit — if one child throws, the rest are cancelled. `supervisorScope` isolates failures so one failing child does not cancel its siblings.

| Concern | `coroutineScope` | `supervisorScope` |
|---------|------------------|-------------------|
| One child fails | cancels all other children and rethrows | other children keep running |
| Use when | all results are needed together (fail fast) | children are independent (e.g. per-connection handlers) |
| Exception surfaces | at the scope call site | handled per-child; install a `CoroutineExceptionHandler` for `launch` |

Cancellation is cooperative: a coroutine only stops at a suspension point. Never catch `CancellationException` and swallow it — always rethrow it, or use `try/finally`/`ensureActive()` for cleanup. Blocking calls inside a coroutine (e.g. `Thread.sleep`, blocking IO on `Dispatchers.Default`) freeze a shared thread; wrap them in `withContext(Dispatchers.IO)`.

```kotlin
suspend fun poll() {
    try {
        while (currentCoroutineContext().isActive) {   // check cancellation cooperatively
            doWork()
            delay(1.seconds)                            // suspension point; honours cancellation
        }
    } catch (e: CancellationException) {
        throw e                                         // NEVER swallow cancellation
    } finally {
        releaseResources()                              // finally still runs on cancellation
    }
}
```

## Flow, StateFlow, and SharedFlow

`Flow<T>` is a cold asynchronous stream: nothing runs until a terminal operator (`collect`) subscribes, and each collector re-runs the producer. Build with the `flow { }` builder, transform with `map`/`filter`/`transform`, move the *producer* to another dispatcher with `flowOn` (it affects upstream only), and control backpressure with `buffer`/`conflate`. Bridge callback APIs with `callbackFlow`.

```kotlin
import kotlinx.coroutines.flow.*

fun priceUpdates(symbol: String): Flow<Price> = flow {
    while (true) {
        emit(api.fetchPrice(symbol))   // cold: runs per collector
        delay(1.seconds)
    }
}.flowOn(Dispatchers.IO)               // producer runs on IO; collector stays where it is

suspend fun show(symbol: String) {
    priceUpdates(symbol)
        .filter { it.value > 0 }
        .conflate()                    // drop intermediate values if the collector is slow
        .collect { render(it) }
}
```

`StateFlow` and `SharedFlow` are *hot* — they emit regardless of collectors. Choose between them with this table.

| | `StateFlow<T>` | `SharedFlow<T>` |
|--|----------------|-----------------|
| Holds a current value | yes (`.value` always readable) | no (configurable replay cache) |
| Emits initial value to new collectors | yes | only the replay cache, if any |
| Conflates | yes (drops intermediate, keeps latest) | configurable |
| Use for | observable state (UI state, current config) | one-off events (navigation, toasts, signals) |

```kotlin
class CounterViewModel(scope: CoroutineScope) {
    private val _state = MutableStateFlow(0)
    val state: StateFlow<Int> = _state.asStateFlow()   // expose read-only

    private val _events = MutableSharedFlow<String>()  // no initial value; discrete events
    val events: SharedFlow<String> = _events.asSharedFlow()

    fun increment() { _state.update { it + 1 } }       // atomic update, not _state.value = _state.value + 1
}
```

Prefer `Flow` over `Channel` for streams of values; reach for a `Channel` only when you need a genuine hand-off queue between coroutines (producer/consumer with distinct lifecycles). Expose cold `Flow` from repositories and hot `StateFlow` from ViewModels/state holders. Test coroutines with `runTest` and virtual time — never with real `delay`.

## Collections and sequences

Kotlin's `List`/`Set`/`Map` interfaces are read-only *views*; `MutableList` etc. add mutation. Default to the read-only type in signatures and return types, and construct with `listOf`/`mapOf` or the `buildList`/`buildMap`/`buildSet` builders when you need to accumulate imperatively.

```kotlin
fun activeUsers(all: List<User>): List<User> =        // read-only in and out
    all.filter { it.active }

val config: Map<String, String> = buildMap {          // build imperatively, expose read-only
    put("host", "localhost")
    put("scheme", if (sslEnabled) "https" else "http")
}
```

Eager collection operators (`map`, `filter`, `groupBy`, `associate`, `partition`, `flatMap`) allocate a new collection at each step. For long chains over large inputs, switch to a `Sequence` so elements flow through all steps lazily with one pass and no intermediate lists. For short chains or small collections, plain collection operators are simpler and faster (no lazy-iterator overhead).

| Use | Collection operators | `Sequence` (`.asSequence()`) |
|-----|----------------------|------------------------------|
| Chain length | short (1–2 steps) | long (3+ transformations) |
| Data size | small/bounded | large or unknown |
| Evaluation | eager, materialized each step | lazy, single pass, terminal-triggered |
| Short-circuit (`first`, `take`) | processes whole collection first | stops early |

```kotlin
val firstBigEven = numbers.asSequence()
    .map { it * 2 }
    .filter { it > 1_000 }
    .first()          // lazy: stops at the first match, never processes the rest
```

Do not use Java streams (`.stream().map(...).collect(...)`) in Kotlin — Kotlin sequences and collection operators are the idiomatic, more concise equivalent. (Note for Compose UIs: a raw `List` crossing a composable boundary is treated as *unstable* by the Compose compiler — see Part II for `kotlinx.collections.immutable`.)

## Type system, generics, and variance

Declare variance at the declaration site with `out` (covariant producer) and `in` (contravariant consumer). Use `where` clauses and upper bounds for multiple constraints, and star projections `<*>` when the type argument is unknown and irrelevant.

```kotlin
interface Producer<out T> { fun next(): T }            // out: T only in output position
interface Consumer<in T> { fun accept(value: T) }      // in: T only in input position

fun <T> copy(from: Producer<out T>, to: Consumer<in T>) {
    to.accept(from.next())
}

fun <T> firstSorted(items: List<T>): T where T : Comparable<T>, T : CharSequence =
    items.sorted().first()
```

Reified type parameters (only on `inline` functions) let generic code inspect `T` at runtime, which erasure normally forbids.

```kotlin
inline fun <reified T> List<*>.filterIsInstanceTyped(): List<T> = filterIsInstance<T>()
```

## Error handling

Kotlin has no checked exceptions — nothing forces a caller to handle a thrown exception, so use exceptions for genuinely exceptional, unrecoverable conditions and model *expected* failures as data in a sealed hierarchy or `Result<T>`. Use the standard precondition functions to fail fast with clear intent.

```kotlin
fun withdraw(account: Account, amount: Cents) {
    require(amount.amount > 0) { "amount must be positive" }        // IllegalArgumentException: bad argument
    check(account.isOpen) { "account ${account.id} is closed" }     // IllegalStateException: bad state
    val remaining = account.balance - amount.amount
    if (remaining < 0) error("insufficient funds")                  // throws IllegalStateException
}
```

For an operation whose failure is a normal outcome, return a sealed result type — it forces the caller to handle both branches at compile time and is clearer than `try/catch` for control flow.

```kotlin
sealed interface Parsed<out T> {
    data class Ok<T>(val value: T) : Parsed<T>
    data class Error(val message: String) : Parsed<Nothing>   // Nothing: no value on the error path
}

fun parsePort(text: String): Parsed<Int> {
    val n = text.toIntOrNull() ?: return Parsed.Error("not a number: $text")
    return if (n in 1..65535) Parsed.Ok(n) else Parsed.Error("out of range: $n")
}
```

Use `runCatching`/`kotlin.Result` to wrap a call that may throw when integrating with a throwing API, but do not let `Result` leak across module boundaries as a public contract, and be careful: `runCatching` catches `Throwable`, so re-throw `CancellationException` inside coroutines.

| Model errors as… | When |
|------------------|------|
| Sealed result type | expected, recoverable, domain-meaningful failures the caller must handle |
| Exceptions (`require`/`check`/`throw`) | programming errors, violated invariants, unrecoverable conditions |
| `Result<T>` / `runCatching` | wrapping a throwing third-party/IO call locally; functional pipelines |

## Kotlin Multiplatform

KMP (Stable) shares Kotlin code across targets. Put common, platform-agnostic code in `commonMain`; put platform implementations in `jvmMain`, `iosMain`, etc. Common code can only use multiplatform libraries and `expect`/`actual` declarations; platform source sets can use platform libraries. The default hierarchy template auto-creates intermediate source sets (`appleMain`, `nativeMain`) so you can share code across a subset of targets.

```kotlin
// build.gradle.kts
plugins {
    kotlin("multiplatform") version "2.4.0"
}

kotlin {
    jvm()
    iosArm64()
    iosSimulatorArm64()
    applyDefaultHierarchyTemplate()   // creates commonMain -> appleMain -> iosMain, etc.

    sourceSets {
        commonMain.dependencies {
            implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
        }
    }
}
```

Declare platform-specific behaviour with `expect`/`actual`. Prefer plain interfaces in `commonMain` with injected platform implementations when you don't need language-level `expect`/`actual`.

```kotlin
// commonMain
expect fun randomUuid(): String

// jvmMain
import java.util.UUID
actual fun randomUuid(): String = UUID.randomUUID().toString()

// iosMain
import platform.Foundation.NSUUID
actual fun randomUuid(): String = NSUUID().UUIDString()
```

For a common UUID that needs no `expect`/`actual`, prefer the stdlib `kotlin.uuid.Uuid`, stabilized in 2.4 (its V4/V7 *generator* functions remain experimental). Compose Multiplatform 1.11.1 shares UI across Android, iOS (Stable since 1.8.0), desktop, and web (Beta since 1.9.0) — treat the web target as Beta and do not depend on it for production-critical UI without validation.

## Serialization

Use `kotlinx.serialization` — it is reflection-free, multiplatform, and compile-time safe. Annotate types with `@Serializable`, apply the `plugin.serialization` compiler plugin, and configure a reusable `Json` instance. Reach for it over Jackson/Gson for any new Kotlin code, especially multiplatform. (It is also the serialization mechanism for Navigation 3 keys on Android TV — see Part II.)

```kotlin
// build.gradle.kts
plugins {
    kotlin("jvm") version "2.4.0"
    kotlin("plugin.serialization") version "2.4.0"
}
dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
}
```

```kotlin
import kotlinx.serialization.*
import kotlinx.serialization.json.*

@Serializable
data class ApiUser(
    val id: String,
    @SerialName("full_name") val name: String,   // map JSON key to property name
    val roles: List<String> = emptyList(),        // default used when key absent
)

val json = Json {
    ignoreUnknownKeys = true      // tolerate extra JSON fields
    encodeDefaults = false        // omit properties equal to their default
    explicitNulls = false         // omit null-valued nullable properties
    prettyPrint = true
}

val text = json.encodeToString(ApiUser(id = "1", name = "Ada Lovelace"))
val user = json.decodeFromString<ApiUser>(text)
```

For polymorphic JSON, use a sealed hierarchy — the plugin generates a discriminator automatically, no manual registration needed.

```kotlin
@Serializable
sealed interface Event {
    @Serializable @SerialName("click") data class Click(val x: Int, val y: Int) : Event
    @Serializable @SerialName("scroll") data class Scroll(val delta: Int) : Event
}
// Encodes as {"type":"click","x":10,"y":20}
```

For a custom wire format on one type, implement `KSerializer<T>` and attach it with `@Serializable(with = ...)`; only JSON is a Stable format — CBOR, ProtoBuf and others remain experimental.

## Build tooling: Gradle Kotlin DSL and version catalogs

Use `build.gradle.kts` (Kotlin DSL), a `libs.versions.toml` version catalog for all versions, `jvmToolchain` to pin the JDK for both compilation and execution, and the `compilerOptions {}` DSL. The old `kotlinOptions {}` block was deprecated in Kotlin 2.0 and **removed in 2.2** — never use it. (This applies equally to Android modules; the Android TV catalog and module in Part II build on this foundation.)

```toml
# gradle/libs.versions.toml
[versions]
kotlin = "2.4.0"
coroutines = "1.11.0"
serialization = "1.11.0"
junit = "5.14.4"
mockk = "1.14.3"

[libraries]
kotlinx-coroutines-core = { module = "org.jetbrains.kotlinx:kotlinx-coroutines-core", version.ref = "coroutines" }
kotlinx-coroutines-test = { module = "org.jetbrains.kotlinx:kotlinx-coroutines-test", version.ref = "coroutines" }
kotlinx-serialization-json = { module = "org.jetbrains.kotlinx:kotlinx-serialization-json", version.ref = "serialization" }
junit-jupiter = { module = "org.junit.jupiter:junit-jupiter", version.ref = "junit" }
mockk = { module = "io.mockk:mockk", version.ref = "mockk" }

[plugins]
kotlin-jvm = { id = "org.jetbrains.kotlin.jvm", version.ref = "kotlin" }
kotlin-serialization = { id = "org.jetbrains.kotlin.plugin.serialization", version.ref = "kotlin" }
```

```kotlin
// build.gradle.kts
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin {
    jvmToolchain(21)                       // pins JDK for compile + run
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_21)
        allWarningsAsErrors.set(true)
        progressiveMode.set(true)
    }
}

dependencies {
    implementation(libs.kotlinx.coroutines.core)
    implementation(libs.kotlinx.serialization.json)
    testImplementation(kotlin("test"))
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockk)
}

tasks.test { useJUnitPlatform() }
```

## Testing (general)

Write assertions with `kotlin.test` (multiplatform, delegates to JUnit Platform on JVM) and run on JUnit Jupiter 5.14.4. Test suspend code with `runTest` from `kotlinx-coroutines-test`, which uses virtual time — `delay` is skipped, and you advance the clock with `advanceUntilIdle()`/`advanceTimeBy()`. Inject a `StandardTestDispatcher` (or `UnconfinedTestDispatcher`) rather than real dispatchers so scheduling is deterministic.

```kotlin
import kotlin.test.*
import kotlinx.coroutines.test.*
import kotlinx.coroutines.Dispatchers

class OrderServiceTest {

    @Test
    fun `computes total`() {
        val order = Order("1", listOf(Order.Line("sku", 2)))
        assertEquals(2, order.lines.sumOf { it.quantity })
    }

    @Test
    fun `loads dashboard concurrently`() = runTest {  // virtual time; delays skipped
        val service = ReportService(FakeRepo(), backgroundScope)
        val reports = service.loadDashboard(listOf("a", "b"))
        advanceUntilIdle()
        assertEquals(2, reports.size)
    }
}
```

Mock with **MockK**, the idiomatic Kotlin mocking library — it understands `suspend` functions, coroutines, and final classes (Kotlin classes are final by default). Do **not** reach for Mockito, which is Java-first and fights Kotlin's final-by-default and suspend semantics.

```kotlin
import io.mockk.*

@Test
fun `fetches from repository`() = runTest {
    val repo = mockk<ReportRepository>()
    coEvery { repo.fetch("a") } returns Report("a")   // coEvery for suspend functions
    val service = ReportService(repo, backgroundScope)

    val result = service.load("a")

    assertEquals("a", result.id)
    coVerify(exactly = 1) { repo.fetch("a") }
}
```

Kotest 6.1.11 is a mature alternative test framework (expressive spec styles and property testing); use it when you want its matchers/spec DSL, otherwise `kotlin.test` + JUnit is the default. (Note: the JUnit project has since moved to a 6.x generation; JUnit Jupiter 5.14.4 remains the current 5.x line and the safe default for Kotlin projects on Java 17/21.) TV-specific testing — ViewModel/flow tests with Turbine, Compose D-pad UI tests, and screenshot testing — is covered in Part II.

## Formatting and static analysis

Formatting and linting are two jobs. Pick **one** formatter — ktlint 1.8.0 (opinionated, near-zero config, based on the official Kotlin style) or ktfmt 0.64 (fully deterministic, Google/Meta style, minimal bikeshedding) — and add **detekt** 1.23.8 for static analysis (code smells, complexity, potential bugs). detekt is not a formatter; do not use it to enforce style.

| Tool | Job | Choose when |
|------|-----|-------------|
| ktlint | format + basic style lint | you want the official kotlinlang style with auto-fix and minimal setup |
| ktfmt | format only | you want strictly deterministic formatting and to end style debates |
| detekt | static analysis (bugs, smells, complexity) | always, alongside a formatter |

```kotlin
// build.gradle.kts — detekt (stable 1.x)
plugins {
    id("io.gitlab.arturbosch.detekt") version "1.23.8"
}

detekt {
    buildUponDefaultConfig = true
    config.setFrom(files("$rootDir/config/detekt/detekt.yml"))
    baseline = file("$rootDir/config/detekt/baseline.xml")
}
```

```yaml
# config/detekt/detekt.yml
complexity:
  LongMethod:
    threshold: 60
  TooManyFunctions:
    active: false
style:
  MaxLineLength:
    maxLineLength: 120
  ForbiddenComment:
    active: true
```

Configure ktlint style through `.editorconfig`:

```editorconfig
# .editorconfig
[*.{kt,kts}]
ktlint_code_style = ktlint_official
max_line_length = 120
indent_size = 4
insert_final_newline = true
```

### Compose-specific rules (for Compose / Compose for TV modules)

In Compose modules, add the Compose rule set (`io.nlopez.compose.rules`) to catch Compose-specific mistakes (unstable params, missing `Modifier` param, emitting content from a function that returns a value). detekt can wrap ktlint if you want a single tool.

```kotlin
// build.gradle.kts (detekt with Compose rules)
plugins { id("io.gitlab.arturbosch.detekt") version "1.23.8" }

dependencies {
    detektPlugins("io.nlopez.compose.rules:detekt:0.4.26")
}

detekt {
    buildUponDefaultConfig = true
    config.setFrom(files("$rootDir/config/detekt/detekt.yml"))
    autoCorrect = true
}
```

Configure ktlint to allow PascalCase composable function names via `.editorconfig`:

```
[*.{kt,kts}]
ktlint_function_naming_ignore_when_annotated_with = Composable
```

Keep Android Lint on in CI for Android/TV modules. It flags TV-relevant issues such as a missing banner or a hard touchscreen requirement.

## Java interoperability

Kotlin sees Java types whose nullability is unknown as *platform types* (`String!`), which bypass null checks — annotate Java APIs (or the boundary) and treat their results defensively, converting to a proper nullable/non-null at the boundary. Control the generated bytecode surface for Java callers with annotations.

```kotlin
class MathUtils {
    companion object {
        @JvmStatic fun clamp(v: Int, lo: Int, hi: Int): Int = v.coerceIn(lo, hi) // callable as MathUtils.clamp
    }
}

class Point(@JvmField val x: Int, @JvmField val y: Int)   // exposes a plain field, no getter

@JvmOverloads                                              // generates Java overloads for defaults
fun connect(host: String, port: Int = 5432): Connection = TODO()

@JvmName("filterValid")                                    // avoid JVM signature clashes
fun List<String>.filterValidStrings(): List<String> = filter { it.isNotBlank() }
```

Kotlin lambdas convert automatically to Java single-abstract-method interfaces (SAM conversion), so you pass a lambda where Java expects a `Runnable`/`Callable`.

## Idiomatic API design

Prefer top-level functions and properties over gratuitous `companion object` and utility classes — Kotlin has no requirement that code live in a class. Use `object` for singletons and stateless helpers. Use `internal` to keep declarations visible within a module but out of the public API. Reserve `companion object` for factory functions and constants tied to a type.

```kotlin
object Currencies {                        // stateless singleton
    val supported: Set<String> = setOf("USD", "EUR", "GBP")
}

class HttpClient private constructor(val baseUrl: String) {
    companion object {                     // factory + type-scoped constants
        const val DEFAULT_TIMEOUT_MS = 30_000
        fun create(baseUrl: String): HttpClient = HttpClient(baseUrl)
    }
}

internal fun normalizeUrl(raw: String): String = raw.trimEnd('/')  // module-private helper
```

Build type-safe DSLs with builder functions taking a receiver lambda, and mark the builder scope with `@DslMarker` to stop inner blocks from accidentally calling outer-scope members.

```kotlin
@DslMarker
annotation class HtmlDsl

@HtmlDsl
class Table {
    private val rows = mutableListOf<Row>()
    fun row(block: Row.() -> Unit) { rows.add(Row().apply(block)) }
    override fun toString() = rows.joinToString("\n")
}

@HtmlDsl
class Row {
    private val cells = mutableListOf<String>()
    fun cell(text: String) { cells.add(text) }
    override fun toString() = cells.joinToString(" | ")
}

fun table(block: Table.() -> Unit): Table = Table().apply(block)

val t = table {
    row { cell("Name"); cell("Age") }
    row { cell("Ada"); cell("36") }
}
```

Use extension functions to add focused behaviour to types you don't own, but keep them cohesive and discoverable — don't scatter unrelated extensions across the codebase.

---

# Part II — Jetpack Compose for TV (androidx.tv)

Everything in Part I applies unchanged on Android TV. This part covers what is TV-specific: project setup, D-pad focus, the TV Material components, the TV architecture, media, navigation, DI, and TV testing.

## Project setup: manifest, Gradle, version catalog

### AndroidManifest — what makes an app a TV app

```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <!-- TVs have no touchscreen; without this the Play Store hides the app from TV. -->
    <uses-feature
        android:name="android.hardware.touchscreen"
        android:required="false" />

    <!-- Declares this is a TV (Leanback) app. required="true" = TV-only;
         use required="false" if the same APK also targets phones. -->
    <uses-feature
        android:name="android.software.leanback"
        android:required="true" />

    <application
        android:banner="@drawable/tv_banner"
        android:icon="@mipmap/ic_launcher"
        android:label="@string/app_name"
        android:theme="@style/Theme.App">

        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:screenOrientation="landscape"
            android:configChanges="keyboard|keyboardHidden|navigation">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <!-- LEANBACK_LAUNCHER makes the app appear on the TV home screen. -->
                <category android:name="android.intent.category.LEANBACK_LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

The `android:banner` is mandatory for the Leanback launcher entry. Per Android's "Create and run a TV app" guidance, use an xhdpi drawable sized **320 × 180 px** (placed in `drawable-xhdpi`), and text must be included in the image. TV apps are landscape-only and single-Activity. Do not add `android.intent.category.LAUNCHER` unless the same APK also ships to phones.

### `gradle/libs.versions.toml`

```toml
[versions]
kotlin = "2.4.0"
agp = "8.13.0"
ksp = "2.4.0-2.0.2"
composeBom = "2026.06.00"
tvMaterial = "1.1.0"
tvFoundation = "1.0.0"
activityCompose = "1.13.0"
lifecycle = "2.11.0"
hilt = "2.57.1"
hiltNavigationCompose = "1.2.0"
navigation3 = "1.0.1"
lifecycleVmNav3 = "1.0.0"
kotlinxSerialization = "1.9.0"
media3 = "1.10.1"
coil = "3.1.0"
kotlinxCollectionsImmutable = "0.4.0"
coroutines = "1.10.2"
turbine = "1.2.1"
roborazzi = "1.44.0"

[libraries]
compose-bom = { module = "androidx.compose:compose-bom", version.ref = "composeBom" }
compose-ui = { module = "androidx.compose.ui:ui" }
compose-ui-tooling = { module = "androidx.compose.ui:ui-tooling" }
compose-ui-tooling-preview = { module = "androidx.compose.ui:ui-tooling-preview" }
compose-ui-test-junit4 = { module = "androidx.compose.ui:ui-test-junit4" }
compose-ui-test-manifest = { module = "androidx.compose.ui:ui-test-manifest" }
compose-foundation = { module = "androidx.compose.foundation:foundation" }
tv-material = { module = "androidx.tv:tv-material", version.ref = "tvMaterial" }
tv-foundation = { module = "androidx.tv:tv-foundation", version.ref = "tvFoundation" }
activity-compose = { module = "androidx.activity:activity-compose", version.ref = "activityCompose" }
lifecycle-runtime-compose = { module = "androidx.lifecycle:lifecycle-runtime-compose", version.ref = "lifecycle" }
lifecycle-viewmodel-compose = { module = "androidx.lifecycle:lifecycle-viewmodel-compose", version.ref = "lifecycle" }
hilt-android = { module = "com.google.dagger:hilt-android", version.ref = "hilt" }
hilt-compiler = { module = "com.google.dagger:hilt-compiler", version.ref = "hilt" }
hilt-navigation-compose = { module = "androidx.hilt:hilt-navigation-compose", version.ref = "hiltNavigationCompose" }
navigation3-runtime = { module = "androidx.navigation3:navigation3-runtime", version.ref = "navigation3" }
navigation3-ui = { module = "androidx.navigation3:navigation3-ui", version.ref = "navigation3" }
lifecycle-viewmodel-navigation3 = { module = "androidx.lifecycle:lifecycle-viewmodel-navigation3", version.ref = "lifecycleVmNav3" }
kotlinx-serialization-json = { module = "org.jetbrains.kotlinx:kotlinx-serialization-json", version.ref = "kotlinxSerialization" }
media3-exoplayer = { module = "androidx.media3:media3-exoplayer", version.ref = "media3" }
media3-ui-compose = { module = "androidx.media3:media3-ui-compose", version.ref = "media3" }
media3-session = { module = "androidx.media3:media3-session", version.ref = "media3" }
coil-compose = { module = "io.coil-kt.coil3:coil-compose", version.ref = "coil" }
coil-network-okhttp = { module = "io.coil-kt.coil3:coil-network-okhttp", version.ref = "coil" }
kotlinx-collections-immutable = { module = "org.jetbrains.kotlinx:kotlinx-collections-immutable", version.ref = "kotlinxCollectionsImmutable" }
coroutines-test = { module = "org.jetbrains.kotlinx:kotlinx-coroutines-test", version.ref = "coroutines" }
turbine = { module = "app.cash.turbine:turbine", version.ref = "turbine" }

[plugins]
android-application = { id = "com.android.application", version.ref = "agp" }
kotlin-android = { id = "org.jetbrains.kotlin.android", version.ref = "kotlin" }
kotlin-compose = { id = "org.jetbrains.kotlin.plugin.compose", version.ref = "kotlin" }
kotlin-serialization = { id = "org.jetbrains.kotlin.plugin.serialization", version.ref = "kotlin" }
ksp = { id = "com.google.devtools.ksp", version.ref = "ksp" }
hilt = { id = "com.google.dagger.hilt.android", version.ref = "hilt" }
```

### App module `build.gradle.kts`

```kotlin
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)        // Compose compiler plugin (in Kotlin since 2.0)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.hilt)
}

android {
    namespace = "com.example.tv"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.example.tv"
        minSdk = 23
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
    }

    buildFeatures { compose = true }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
}

// Kotlin compiler options — compilerOptions {}, never the removed kotlinOptions {} block (see Part I).
kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// Compose compiler metrics/reports (write to build/compose-metrics).
composeCompiler {
    // Uncomment when profiling recomposition/stability:
    // metricsDestination = layout.buildDirectory.dir("compose-metrics")
    // reportsDestination = layout.buildDirectory.dir("compose-reports")
    // stabilityConfigurationFile = rootProject.layout.projectDirectory.file("stability_config.conf")
}

dependencies {
    val composeBom = platform(libs.compose.bom)
    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.compose.foundation)
    implementation(libs.tv.material)
    implementation(libs.tv.foundation)          // only if you need TvImeOptions

    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.viewmodel.compose)

    implementation(libs.navigation3.runtime)
    implementation(libs.navigation3.ui)
    implementation(libs.lifecycle.viewmodel.navigation3)
    implementation(libs.kotlinx.serialization.json)

    implementation(libs.hilt.android)
    ksp(libs.hilt.compiler)
    implementation(libs.hilt.navigation.compose)

    implementation(libs.media3.exoplayer)
    implementation(libs.media3.ui.compose)
    implementation(libs.media3.session)

    implementation(libs.coil.compose)
    implementation(libs.coil.network.okhttp)
    implementation(libs.kotlinx.collections.immutable)

    testImplementation(libs.coroutines.test)
    testImplementation(libs.turbine)
    androidTestImplementation(libs.compose.ui.test.junit4)
    debugImplementation(libs.compose.ui.test.manifest)
}
```

**Why KSP, not KAPT:** KAPT runs the deprecated K1 compiler; KSP2 is the default and the only path compatible with Kotlin 2.4 and AGP 9. Never add the `kotlin-kapt` plugin to a new project. **Why the compose plugin:** since Kotlin 2.0 the Compose compiler ships with Kotlin as `org.jetbrains.kotlin.plugin.compose` and is versioned with Kotlin — do not add the old standalone `androidx.compose.compiler:compiler` extension. Strong skipping is on by default, so stable, skippable composables are skipped automatically during recomposition.

## D-pad focus management — the core discipline

Focus is not application state you own; it is a runtime property the Compose focus system tracks, and you steer it with modifiers. On TV every navigable element must be focusable, must render a visible focus state, and must participate in a sane traversal order.

### The three rules

1. **Never make a bare element `clickable` on TV.** Use a TV `Surface`, `Button`, `Card`, or `ListItem`. They are focusable, handle the D-pad center (`DPAD_CENTER`/`ENTER`) as a click, and render focus indication. A `Modifier.clickable` `Box` is focusable but has no TV focus visuals, so users can't see where they are.
2. **Group related focusables** with `Modifier.focusGroup()` so directional navigation and restoration treat a row/column as a unit.
3. **Restore focus** on every scrollable/navigable container with `Modifier.focusRestorer()` so returning to a screen re-focuses the last item, not item zero.

### `focusRestorer` — stable, and the single highest-value modifier on TV

`Modifier.focusRestorer(fallback: FocusRequester = FocusRequester.Default)` is **stable**: per the Compose UI release notes, "Multiple Focus APIs are now stable, including `Modifier.focusRestorer()` and `onEnter` and `onExit` FocusProperties (I6e667)," landing in the Jetpack Compose August '25 release (core modules 1.9). It saves the last-focused child when focus leaves the group and restores it on re-entry.

```kotlin
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRestorer

@Composable
fun MovieRow(movies: ImmutableList<Movie>, onClick: (Movie) -> Unit) {
    val firstItem = remember { FocusRequester() }
    LazyRow(
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(horizontal = 48.dp),
        // If restoration fails, focus falls back to firstItem.
        modifier = Modifier.focusRestorer(firstItem),
    ) {
        itemsIndexed(movies, key = { _, m -> m.id }) { index, movie ->
            MovieCard(
                movie = movie,
                onClick = { onClick(movie) },
                modifier = if (index == 0) Modifier.focusRequester(firstItem) else Modifier,
            )
        }
    }
}
```

**Behavioral gotcha (verbatim from the Compose UI release notes):** "FocusRestorer no longer pins the previously focused item. Users should use a key to ensure that the previously focused item has the same composition hash, so that focus is successfully restored. (I4203b, b/330696779)." So you **must** give lazy items stable `key`s (as above). Without keys, the previously focused item can be recomposed with a different hash and restoration silently fails.

### Requesting initial focus safely

Calling `requestFocus()` on a `FocusRequester` not attached to any node crashes. Attach first, request inside a `LaunchedEffect`:

```kotlin
@Composable
fun DetailsScreen(onPlay: () -> Unit) {
    val playButton = remember { FocusRequester() }
    LaunchedEffect(Unit) { playButton.requestFocus() }

    Button(
        onClick = onPlay,
        modifier = Modifier.focusRequester(playButton),
    ) { Text("Play") }
}
```

### Directing focus with `focusProperties`

Use `focusProperties` to redirect focus on entry/exit or to constrain directional movement. Modifier order matters — `focusProperties` must come before `focusGroup`.

```kotlin
val tabFocusRequesters = remember { List(tabs.size) { FocusRequester() } }

TabRow(
    selectedTabIndex = selectedIndex,
    modifier = Modifier
        .focusProperties {
            // Entering the TabRow lands on the selected tab, not tab 0.
            onEnter = { tabFocusRequesters[selectedIndex].requestFocus() }
        }
        .focusGroup(),
) {
    tabs.forEachIndexed { i, tab ->
        Tab(
            selected = i == selectedIndex,
            onFocus = { selectedIndex = i },   // TV tabs select on focus, not click
            modifier = Modifier.focusRequester(tabFocusRequesters[i]),
        ) { Text(tab.title, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) }
    }
}
```

To make a node temporarily unfocusable: `Modifier.focusProperties { canFocus = false }`. To block movement in a direction: assign `FocusRequester.Cancel` to `right`/`left`/`up`/`down`.

### Observing focus for custom visuals

When you build a custom focusable (rare — prefer `Surface`), track focus through the interaction source, not ad-hoc booleans:

```kotlin
val interactionSource = remember { MutableInteractionSource() }
val isFocused by interactionSource.collectIsFocusedAsState()
val scale by animateFloatAsState(if (isFocused) 1.1f else 1f, label = "focusScale")

Box(
    modifier = Modifier
        .graphicsLayer { scaleX = scale; scaleY = scale }
        .focusable(interactionSource = interactionSource)
)
```

## Lists and grids: standard foundation + TV pivot scrolling

There is no TV lazy list anymore. Use `LazyColumn`, `LazyRow`, `LazyVerticalGrid`, and `LazyHorizontalGrid` from `androidx.compose.foundation.lazy`. The TV "pivot" behavior (keeping the focused item anchored at a fixed screen position while content scrolls under it) is supplied automatically: `LocalBringIntoViewSpec` resolves to a pivot spec on devices reporting `FEATURE_LEANBACK`, and to the default on phones. You get correct TV scrolling with **no extra code** — a plain `LazyRow` of focusable `Card`s pivots correctly on a TV.

The canonical TV home screen is a vertical list of horizontal rows:

```kotlin
@Composable
fun BrowseScreen(rows: ImmutableList<CatalogRow>, onSelect: (Movie) -> Unit) {
    LazyColumn(
        verticalArrangement = Arrangement.spacedBy(24.dp),
        contentPadding = PaddingValues(vertical = 48.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        items(rows, key = { it.id }) { row ->
            Column {
                Text(
                    text = row.title,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.padding(start = 48.dp, bottom = 12.dp),
                )
                MovieRow(row.movies, onSelect)   // the focusRestorer LazyRow above
            }
        }
    }
}
```

If you need to override the pivot position (for example, anchor the focused item at 25% from the leading edge), provide a custom `BringIntoViewSpec`:

```kotlin
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun PivotRow(content: LazyListScope.() -> Unit) {
    val pivotSpec = remember {
        object : BringIntoViewSpec {
            override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
                val target = containerSize * 0.25f
                return offset - target
            }
        }
    }
    CompositionLocalProvider(LocalBringIntoViewSpec provides pivotSpec) {
        LazyRow(content = content)
    }
}
```

## TV Material components

Import all of these from `androidx.tv.material3`. They differ from phone Material 3 by exposing focus-state parameters (`scale`, `glow`, `border`, per-state `colors`) instead of touch ripples.

### `Surface` — the focusable primitive everything is built on

`Surface` is the base for any custom focusable card/tile. The clickable overload is focusable, handles D-pad center and long-press, and animates scale/glow/border between rest, focused, and pressed states.

```kotlin
import androidx.tv.material3.Surface
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.Border
import androidx.tv.material3.Glow

@Composable
fun MovieCard(movie: Movie, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        onClick = onClick,
        onLongClick = { /* context menu */ },
        shape = ClickableSurfaceDefaults.shape(shape = RoundedCornerShape(12.dp)),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1.1f),
        colors = ClickableSurfaceDefaults.colors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
            focusedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
        border = ClickableSurfaceDefaults.border(
            focusedBorder = Border(
                border = BorderStroke(3.dp, MaterialTheme.colorScheme.onSurface),
                shape = RoundedCornerShape(12.dp),
            ),
        ),
        glow = ClickableSurfaceDefaults.glow(
            focusedGlow = Glow(elevationColor = Color.Black.copy(alpha = 0.5f), elevation = 8.dp),
        ),
        modifier = modifier.width(220.dp).aspectRatio(2f / 3f),
    ) {
        AsyncImage(
            model = movie.posterUrl,
            contentDescription = movie.title,
            contentScale = ContentScale.Crop,
            modifier = Modifier.fillMaxSize(),
        )
    }
}
```

`Surface` has three overloads: non-interactive (no `onClick`), clickable (`onClick`), and selectable/toggleable (`selected`/`onCheckedChange`). Prefer them over hand-rolled focusables. The non-clickable `Surface` uses `SurfaceDefaults`; the clickable one uses `ClickableSurfaceDefaults`.

### Buttons

```kotlin
import androidx.tv.material3.Button
import androidx.tv.material3.OutlinedButton
import androidx.tv.material3.Text

Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    Button(onClick = onPlay) {
        Icon(Icons.Default.PlayArrow, contentDescription = null)
        Spacer(Modifier.width(8.dp))
        Text("Play")
    }
    OutlinedButton(onClick = onAddToList) { Text("My List") }
}
```

`Button`, `OutlinedButton`, `IconButton`, `OutlinedIconButton`, and `WideButton` all accept `onLongClick` and per-state `scale`/`glow`/`border`/`colors` through their `*Defaults`. `WideButton` is the full-width list-style button used on settings screens.

### Cards

`Card`, `ClassicCard`, `CompactCard`, `WideClassicCard`, `StandardCardContainer`, and `WideCardContainer` are the content-tile family. `ClassicCard` stacks image over a title/subtitle block; `CompactCard` overlays text on the image; the `*Container` variants place the label outside the focusable image area.

```kotlin
import androidx.tv.material3.ClassicCard

ClassicCard(
    onClick = onClick,
    image = {
        AsyncImage(
            model = movie.posterUrl,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.fillMaxWidth().aspectRatio(16f / 9f),
        )
    },
    title = { Text(movie.title) },
    subtitle = { Text(movie.year.toString()) },
    modifier = Modifier.width(260.dp),
)
```

### `ModalNavigationDrawer` — primary side navigation

The TV navigation pattern is a left-edge drawer that expands from icons to labels when focused. Use `ModalNavigationDrawer` (draws over content) or `NavigationDrawer` (reserves space). The drawer content is a `NavigationDrawerScope` lambda receiving the current `DrawerValue`.

```kotlin
import androidx.tv.material3.ModalNavigationDrawer
import androidx.tv.material3.NavigationDrawerItem
import androidx.tv.material3.rememberDrawerState
import androidx.tv.material3.DrawerValue

@Composable
fun AppScaffold(selected: Destination, onSelect: (Destination) -> Unit, content: @Composable () -> Unit) {
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = { drawerValue ->
            Column(
                modifier = Modifier
                    .fillMaxHeight()
                    .padding(12.dp)
                    .selectableGroup(),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Destination.entries.forEach { dest ->
                    NavigationDrawerItem(
                        selected = dest == selected,
                        onClick = { onSelect(dest) },
                        leadingContent = { Icon(dest.icon, contentDescription = null) },
                    ) { Text(dest.label) }
                }
            }
        },
    ) { content() }
}
```

### `TabRow` for top navigation

TV tabs select on **focus**, not click — moving the D-pad along the row changes the active tab. Wire `onFocus` to update selection and drive content.

```kotlin
import androidx.tv.material3.TabRow
import androidx.tv.material3.Tab

var selectedIndex by remember { mutableIntStateOf(0) }
TabRow(selectedTabIndex = selectedIndex) {
    categories.forEachIndexed { i, category ->
        Tab(
            selected = i == selectedIndex,
            onFocus = { selectedIndex = i },
        ) {
            Text(category.name, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
        }
    }
}
```

### `ListItem`, `Checkbox`, `Switch`, `RadioButton`

`ListItem`/`DenseListItem` are the focusable rows for settings and detail lists; `Checkbox`, `Switch`, and `RadioButton` are the stable selection controls. All render TV focus indication.

```kotlin
import androidx.tv.material3.ListItem
import androidx.tv.material3.Switch

ListItem(
    selected = false,
    onClick = { onToggle(!enabled) },
    headlineContent = { Text("Subtitles") },
    trailingContent = { Switch(checked = enabled, onCheckedChange = null) },
)
```

### Immersive / featured content (build it yourself)

`ImmersiveList` was removed. The pattern is a full-bleed background driven by the focused item in a `LazyRow`:

```kotlin
@Composable
fun FeaturedBanner(items: ImmutableList<Movie>, onClick: (Movie) -> Unit) {
    var focusedIndex by remember { mutableIntStateOf(0) }
    Box(Modifier.fillMaxWidth().height(420.dp)) {
        AnimatedContent(targetState = items[focusedIndex], label = "hero") { movie ->
            AsyncImage(
                model = movie.backdropUrl,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        }
        LazyRow(
            modifier = Modifier.align(Alignment.BottomStart).padding(48.dp),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            itemsIndexed(items, key = { _, m -> m.id }) { index, movie ->
                MovieCard(
                    movie = movie,
                    onClick = { onClick(movie) },
                    modifier = Modifier.onFocusChanged { if (it.isFocused) focusedIndex = index },
                )
            }
        }
    }
}
```

### `Carousel` and chips (experimental)

`Carousel` (auto-advancing hero rotator) and the chip family remain `@ExperimentalTvMaterial3Api`. For production, prefer the hand-built featured banner above and `Surface`-based filter tiles. If you do use them, gate the opt-in explicitly and understand the API may change.

## Typography, theme, and the 10-foot UI

Use `androidx.tv.material3.MaterialTheme`, `darkColorScheme`/`lightColorScheme`, and TV `Text`. Design for a viewer ~10 feet away:

- Keep a **5% overscan safe margin** — pad screen content roughly 48 dp horizontally and 27 dp vertically so nothing critical sits at the panel edge.
- Minimum body text ~18 sp; prefer larger. Default to a dark theme — bright full-screen backgrounds are fatiguing on a large panel.
- Landscape only; never assume portrait constraints.

```kotlin
@Composable
fun AppTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Color(0xFF7FCFFF),
            surface = Color(0xFF101418),
            background = Color(0xFF0A0D10),
        ),
        typography = Typography(),
        content = content,
    )
}

@Composable
fun Screen(content: @Composable () -> Unit) {
    Surface(modifier = Modifier.fillMaxSize()) {
        Box(Modifier.padding(horizontal = 48.dp, vertical = 27.dp)) { content() }
    }
}
```

## Architecture: single Activity, ViewModel, unidirectional state

Use one `ComponentActivity`, Compose for all UI, a `ViewModel` per screen exposing a single immutable `StateFlow<UiState>` (applying the coroutines/Flow discipline from Part I: launch from `viewModelScope`, never block, never leak a `GlobalScope` job), and events flowing down as lambdas. Collect state lifecycle-aware with `collectAsStateWithLifecycle()`.

```kotlin
@HiltViewModel
class BrowseViewModel @Inject constructor(
    private val catalog: CatalogRepository,
) : ViewModel() {

    private val _uiState = MutableStateFlow<BrowseUiState>(BrowseUiState.Loading)
    val uiState: StateFlow<BrowseUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            catalog.rows()
                .catch { _uiState.value = BrowseUiState.Error(it.message.orEmpty()) }
                .collect { rows -> _uiState.value = BrowseUiState.Content(rows.toImmutableList()) }
        }
    }
}

@Immutable
sealed interface BrowseUiState {
    data object Loading : BrowseUiState
    data class Content(val rows: ImmutableList<CatalogRow>) : BrowseUiState
    data class Error(val message: String) : BrowseUiState
}

@Composable
fun BrowseRoute(viewModel: BrowseViewModel = hiltViewModel(), onSelect: (Movie) -> Unit) {
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    when (val s = state) {
        BrowseUiState.Loading -> LoadingScreen()
        is BrowseUiState.Content -> BrowseScreen(s.rows, onSelect)
        is BrowseUiState.Error -> ErrorScreen(s.message)
    }
}
```

**Stability and recomposition:** the state class is `@Immutable` and holds `ImmutableList` (from `kotlinx.collections.immutable`), not `List`. A raw `List` is treated as *unstable* by the compiler, defeating skipping and causing every child to recompose. Use `persistentListOf`/`toImmutableList()` for any collection that crosses a composable boundary. Annotate stable-but-not-provably-so classes with `@Stable`/`@Immutable`. Hoist derived values with `derivedStateOf` when a value is computed from other state and read during composition.

### Side effects — pick the right API

| API | Use for |
|---|---|
| `LaunchedEffect(key)` | Start a coroutine tied to composition; re-runs when `key` changes. Initial focus, one-shot loads. |
| `rememberCoroutineScope()` | Launch from a callback (e.g. `onClick`) outside composition. |
| `DisposableEffect(key)` | Setup that needs teardown — register/unregister listeners, **release ExoPlayer**. |
| `produceState` | Convert a non-Compose async source into `State`. |
| `rememberUpdatedState` | Capture the latest lambda/value inside a long-lived effect without restarting it. |

Use `remember` for recomposition-scoped values and `rememberSaveable` for values that must survive configuration changes/process death (e.g. selected tab index).

## Media playback: Media3 + Compose

Media3 (ExoPlayer) is the only supported player; `android.media.MediaPlayer` and the standalone `exoplayer2` are obsolete. Per the Android Developers Blog, "Media3 1.6.0 introduces a new `media3-ui-compose` module that contains functionality for building Compose UIs for playback… a first set of foundational state classes that link to the Player, in addition to some basic composable building blocks." Use `PlayerSurface` from that module — do **not** wrap the old `PlayerView` in `AndroidView` for new code.

```kotlin
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.ui.compose.PlayerSurface
import androidx.media3.ui.compose.SURFACE_TYPE_SURFACE_VIEW
import androidx.media3.ui.compose.state.rememberPresentationState

@OptIn(UnstableApi::class)
@Composable
fun VideoPlayer(url: String, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val player = remember {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            prepare()
            playWhenReady = true
        }
    }
    // Release on leaving composition — the classic ExoPlayer leak if omitted.
    DisposableEffect(Unit) { onDispose { player.release() } }

    val presentationState = rememberPresentationState(player)

    Box(modifier.fillMaxSize().background(Color.Black)) {
        PlayerSurface(
            player = player,
            surfaceType = SURFACE_TYPE_SURFACE_VIEW,   // best for TV video
            modifier = Modifier
                .resizeWithContentScale(ContentScale.Fit, presentationState.videoSizeDp)
                .fillMaxSize(),
        )
        if (presentationState.coverSurface) {
            // Hide the surface until the first frame is ready.
            Box(Modifier.matchParentSize().background(Color.Black))
        }
    }
}
```

`PlayerSurface` is still marked `@UnstableApi` (confirmed in the androidx/media source: `@UnstableApi @Composable fun PlayerSurface(player: Player?, modifier: Modifier = Modifier, surfaceType: @SurfaceType Int = SURFACE_TYPE_SURFACE_VIEW)`), so annotate the call site with `@OptIn(UnstableApi::class)`. Use `SURFACE_TYPE_SURFACE_VIEW` for video playback on TV (better power/performance and HDR than a `TextureView`). Drive custom D-pad transport controls from Media3's state holders (`rememberPlayPauseButtonState`, etc.), and register a `MediaSession` (`media3-session`) so the system remote, "now playing," and background audio behave correctly. For images, use Coil 3 (`AsyncImage`) with a placeholder to avoid layout shift.

## Navigation

Navigation 3 (`androidx.navigation3`) is stable and Compose-first: the back stack is an observable list *you* own, rendered by `NavDisplay`. The older `androidx.navigation:navigation-compose` (Nav2) is in maintenance mode — use Nav3 for new apps. Keys are `@Serializable` `NavKey`s, serialized with `kotlinx.serialization` (Part I).

```kotlin
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.entry
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.ui.NavDisplay
import kotlinx.serialization.Serializable

@Serializable data object Browse : NavKey
@Serializable data class Details(val movieId: String) : NavKey
@Serializable data class Player(val movieId: String) : NavKey

@Composable
fun AppNavigation() {
    val backStack = rememberNavBackStack(Browse)
    NavDisplay(
        backStack = backStack,
        entryProvider = entryProvider {
            entry<Browse> {
                BrowseRoute(onSelect = { backStack.add(Details(it.id)) })
            }
            entry<Details> { key ->
                DetailsRoute(
                    movieId = key.movieId,
                    onPlay = { backStack.add(Player(key.movieId)) },
                )
            }
            entry<Player> { key -> VideoPlayerRoute(key.movieId) }
        },
    )
}
```

Navigate by mutating the list: `backStack.add(...)` to push, `backStack.removeLastOrNull()` to pop. Scope a Hilt `ViewModel` to a destination with the `lifecycle-viewmodel-navigation3` integration. Combine `focusRestorer()` on each screen's containers with Nav3 so focus returns to the right item after a pop.

## Dependency injection: Hilt

Hilt is the recommended DI for Android and integrates cleanly with Compose ViewModels. Use the `com.google.dagger.hilt.android` plugin and `ksp(hilt-compiler)`.

```kotlin
@HiltAndroidApp
class TvApp : Application()

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { AppTheme { AppNavigation() } }
    }
}

@Module
@InstallIn(SingletonComponent::class)
object DataModule {
    @Provides @Singleton
    fun provideCatalogRepository(api: CatalogApi): CatalogRepository = DefaultCatalogRepository(api)
}
```

Inject ViewModels into composables with `hiltViewModel()` from `androidx.hilt:hilt-navigation-compose`. Note the Compose `hiltViewModel()` APIs were split into `androidx.hilt:hilt-lifecycle-viewmodel-compose` so they no longer transitively pull in navigation. Koin is a viable pure-Kotlin alternative for teams that prefer runtime DI, but Hilt is the default for TV production apps.

## Testing on TV

Build on the general testing setup from Part I (`runTest`, `StandardTestDispatcher`, MockK); this section covers what is TV/Compose-specific.

### ViewModels and flows

Use `kotlinx-coroutines-test` (`runTest`, a `StandardTestDispatcher`) and Turbine for flow assertions. Inject the dispatcher so tests control virtual time.

```kotlin
@Test
fun emitsContentAfterLoading() = runTest {
    val vm = BrowseViewModel(FakeCatalogRepository(sampleRows))
    vm.uiState.test {
        assertEquals(BrowseUiState.Loading, awaitItem())
        val content = awaitItem() as BrowseUiState.Content
        assertEquals(sampleRows.size, content.rows.size)
        cancelAndIgnoreRemainingEvents()
    }
}
```

### Compose UI and D-pad focus

Use `createComposeRule()`, tag nodes with `Modifier.testTag(...)`, and drive the D-pad with key input — this is the TV-specific part agents miss. Prefer the v2 test entry points (now the default): they use `StandardTestDispatcher`, so advance the clock explicitly rather than relying on immediate execution.

```kotlin
@Test
fun dpadRightMovesFocusAlongRow() {
    composeTestRule.setContent { AppTheme { MovieRow(sampleMovies) {} } }

    composeTestRule.onNodeWithTag("movie-0").requestFocus()
    composeTestRule.onNodeWithTag("movie-0").assertIsFocused()

    composeTestRule.onRoot().performKeyInput { pressKey(Key.DirectionRight) }
    composeTestRule.onNodeWithTag("movie-1").assertIsFocused()

    composeTestRule.onRoot().performKeyInput { pressKey(Key.DirectionCenter) }  // D-pad click
}
```

Assert focus with `assertIsFocused()`/`assertIsNotFocused()` and always test that navigating away and back restores focus to the previously focused item — the single most common TV regression.

### Screenshot testing

For a greenfield TV app, **Roborazzi** is the strongest default: it runs on the JVM via Robolectric (no emulator), supports interaction (focus a card, then snapshot), and integrates with Hilt. Paparazzi is faster for pure static single-frame snapshots but cannot interact and is incompatible with Robolectric. Compose Preview Screenshot Testing (Google's built-in tool) is the lowest-effort option for static `@Preview`-based snapshots. Because TV correctness is about *focused* states, favor a tool that can snapshot after injecting focus — i.e. Roborazzi.

---

# Anti-patterns to avoid

## General Kotlin

| Wrong (Java/old-Kotlin habit) | Right (idiomatic Kotlin 2.4) |
|-------------------------------|------------------------------|
| `user!!.name` | `user?.name ?: default` or `requireNotNull(user) { "..." }.name` |
| `values()` on an enum in a loop | `MyEnum.entries` (cached, no allocation) |
| `list.stream().map{}.collect(...)` | `list.map { }` or `list.asSequence().map { }.filter { }` for long chains |
| `GlobalScope.launch { }` | inject a lifecycle-bound `CoroutineScope`; use `coroutineScope { }` |
| `runBlocking { }` in production code | `suspend fun` + structured scope; `runBlocking` only in `main`/tests |
| `catch (e: CancellationException) { /* ignore */ }` | rethrow `CancellationException`; clean up in `finally` |
| `Thread.sleep` / blocking IO on `Dispatchers.Default` | `delay(...)`; wrap blocking calls in `withContext(Dispatchers.IO)` |
| `synchronized` + manual thread pools for concurrency | coroutines, `Mutex`, `StateFlow`, structured scopes |
| `lateinit var config: Config` for a computed value | `val config by lazy { }` |
| `_state.value = _state.value + 1` | `_state.update { it + 1 }` (atomic) |
| Exposing `MutableStateFlow`/`MutableList` publicly | expose `.asStateFlow()` / read-only `List` |
| Mockito for Kotlin classes | MockK (`mockk`, `every`/`coEvery`, `verify`/`coVerify`) |
| `kotlinOptions { jvmTarget = "21" }` | `compilerOptions { jvmTarget.set(JvmTarget.JVM_21) }` |
| `context(Logger)` context receivers | named context parameters: `context(logger: Logger)` |
| Overloaded constructors for optional args | default + named arguments |
| Utility classes full of `@JvmStatic` | top-level functions / `object` |
| Treating Java `String!` platform types as non-null | convert to `String?`/`String` at the boundary; `-Xjsr305=strict` |

## Compose for TV

- **Using `androidx.compose.material3` components on TV.** They have no focus scale/glow/border and often no D-pad affordance. Always import from `androidx.tv.material3`.
- **`Modifier.clickable` on a bare `Box`/`Row` for tiles.** No visible focus state. Use TV `Surface`/`Button`/`Card`/`ListItem`.
- **Reintroducing `TvLazyRow`/`TvLazyColumn`/`TvLazyVerticalGrid`.** Removed. Use foundation `Lazy*`; TV pivot comes from `LocalBringIntoViewSpec` automatically.
- **Any use of Leanback (`androidx.leanback`, `BrowseSupportFragment`, `Presenter`, `ArrayObjectAdapter`).** Legacy; not for new apps.
- **Omitting `focusRestorer()`** on rows/screens — focus jumps to item zero on return.
- **Lazy items without stable `key`s** — breaks focus restoration and recomposition efficiency.
- **`requestFocus()` on an unattached `FocusRequester`** — crashes. Attach via `Modifier.focusRequester`, request inside `LaunchedEffect`.
- **Not releasing `ExoPlayer`** in `DisposableEffect.onDispose` — leaks the player and audio focus.
- **Wrapping the legacy `PlayerView` in `AndroidView`** for new playback UIs — use `PlayerSurface` from `media3-ui-compose`.
- **Passing raw `List`/`Set`/`Map` across composable boundaries** — treated as unstable, defeats skipping. Use `kotlinx.collections.immutable`.
- **KAPT / the standalone Compose compiler extension** — use KSP2 and the `org.jetbrains.kotlin.plugin.compose` plugin.
- **Portrait or touch assumptions** — TV is landscape, remote-driven; declare `touchscreen` not required and lock orientation.
- **Using experimental `Carousel`/chips in production** without accepting API churn — prefer stable components or hand-built equivalents.

---

# Quick reference

## Kotlin language feature floors

| Feature | Version floor |
|---------|---------------|
| K2 compiler default | 2.0 (only frontend since 2.4) |
| `Enum.entries`, `data object` | 1.9 |
| `@JvmInline value class` | 1.5 |
| Guard conditions in `when`, non-local `break`/`continue`, multi-dollar interpolation | 2.2 |
| Stable `Base64`/`HexFormat` | 2.2 |
| Context parameters (stable; context arguments & callable refs still experimental) | 2.4 |
| Explicit backing fields | 2.4 |
| Stable `kotlin.uuid.Uuid` (V4/V7 generators still experimental) | 2.4 |
| Name-based destructuring | still experimental in 2.4 (`-Xname-based-destructuring`) — don't ship it |
| Java bytecode target | up to Java 26 (2.4) |

## Core toolchain

| Job | Library / tool (version) |
|-----|--------------------------|
| Coroutines | kotlinx-coroutines-core 1.11.0 |
| JSON / serialization | kotlinx-serialization-json 1.11.0 |
| Build | Gradle Kotlin DSL + KGP 2.4.0 (Gradle ≤ 9.5.0) |
| Formatter | ktlint 1.8.0 **or** ktfmt 0.64 |
| Static analysis | detekt 1.23.8 (2.0 is alpha; add `io.nlopez.compose.rules` in Compose modules) |
| Testing | kotlin.test + JUnit Jupiter 5.14.4, kotlinx-coroutines-test 1.11.0 |
| Mocking | MockK 1.14.3 (not Mockito) |
| Alternative test framework | Kotest 6.1.11 |
| Multiplatform UI | Compose Multiplatform 1.11.1 (iOS Stable, Web Beta) |

## Compose for TV

| Need | Use | Not |
|---|---|---|
| Focusable tile | `androidx.tv.material3.Surface` / `Card` | `Box` + `clickable` |
| Interactive component set | `androidx.tv.material3.*` | `androidx.compose.material3.*` |
| Horizontal content row | `LazyRow` (foundation) + `focusRestorer()` | `TvLazyRow` (removed) |
| Grid | `LazyVerticalGrid` (foundation) | `TvLazyVerticalGrid` (removed) |
| Restore focus | `Modifier.focusRestorer(fallback)` (stable, 1.9) | manual `saveFocusedChild`/`restoreFocusedChild` glue |
| Initial focus | `FocusRequester` + `LaunchedEffect { requestFocus() }` | bare `requestFocus()` |
| Side navigation | `ModalNavigationDrawer` | Material `NavigationRail` |
| Top navigation | `TabRow` + `Tab(onFocus = …)` | click-driven tabs |
| Featured/immersive | Hand-built `AnimatedContent` + `LazyRow` | `ImmersiveList` (removed) |
| Video | `PlayerSurface` (`media3-ui-compose`) | `PlayerView` in `AndroidView` |
| Navigation | Navigation 3 (`NavDisplay`) | Nav2 (maintenance) / Leanback |
| DI | Hilt + KSP | Dagger by hand / KAPT |
| List collections in state | `ImmutableList` | `List` |
| Annotation processing | KSP2 | KAPT |
| Screenshot tests (with focus) | Roborazzi | Paparazzi (static only) |
