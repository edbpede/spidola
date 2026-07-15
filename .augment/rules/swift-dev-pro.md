---
type: "agent_requested"
description: "Swift 6 + SwiftUI + Observation coding guidelines (language core + app stack)"
---
# Swift 6.3 + SwiftUI + Observation: A Data-Race-Safe, Value-Semantics-First Reference for Coding Agents

Swift 6 is a compile-time-safety language whose defining posture is **static data-race safety**: in the Swift 6 language mode the compiler proves the absence of data races before your program runs, using actor isolation, `Sendable`, and region-based analysis. Combined with SwiftUI and the Observation framework, it forms a fully data-race-safe, macro-driven, single-threaded-by-default application stack that also scales to Linux, Windows, Android, WebAssembly, server, and embedded targets.

When writing on this stack, optimize for:

1. **Value types by default, `let` over `var`, exhaustive pattern matching** — let the compiler's safety checks guide the design rather than fighting them.
2. **Main-actor-by-default, minimal explicit isolation** — stay single-threaded and reach for concurrency only when you have real parallel work, using `@concurrent` deliberately.
3. **`@Observable` + SwiftUI's fine-grained dependency tracking** for UI state — never `ObservableObject`/`@Published`.
4. **`async`/`await` and structured concurrency** for all asynchrony — never completion handlers, `DispatchQueue`/GCD, or Combine.

The biggest way agents write wrong-but-plausible Swift is by importing habits from adjacent ecosystems and older SwiftUI: classes and GCD from Objective-C-era Swift; `class ViewModel: ObservableObject` with `@Published`, `@StateObject`/`@ObservedObject`/`@EnvironmentObject`, Combine pipelines, `NavigationView` push-style navigation, completion-handler `URLSession`, and XCTest for new unit tests; free mutation of shared state from Python/JavaScript; over-reaching for `~Copyable` and manual ownership from Rust. The single highest-value thing to get right is **concurrency and `Sendable`**: most incorrect Swift 6 code is either an isolation mistake, an `@unchecked Sendable` escape hatch that reintroduces the race the compiler just prevented, or a needless hop onto a background executor. This document shows the modern idiom once, well, and version-anchors every feature so you know the floor.

## Stack snapshot

- **Language / toolchain:** Swift 6.3 is the current stable release (announced October 24, 2025; released March 24, 2026; ships with Xcode 26.4, patch releases 6.3.2/6.3.3 available via `swiftly install latest`). Swift 6.2 (released September 15, 2025) shipped the "approachable concurrency" features — single-threaded-by-default, main-actor isolation without explicit annotations — that most new projects should adopt. Swift 6.3 adds the `@c` C-interop attribute, module selectors, library performance-control attributes (`@specialize`, `@inline(always)`, `@export(implementation)`), the first official Swift SDK for Android, and a preview of Swift Build integrated into SwiftPM. Swift 6.4 is in beta (Xcode 27) — do not target it in production.
- **Language mode:** Always the Swift 6 language mode (`-swift-version 6` / `.swiftLanguageMode(.v6)`, `swiftLanguageModes: [.v6]`). Strict concurrency (data-race safety) is on by default in this mode.
- **Build & packaging:** Swift Package Manager, manifest `// swift-tools-version: 6.2` (or `6.3`).
- **UI:** SwiftUI with the Observation framework (`import Observation`, or transitively via `import SwiftUI`); `@Observable` replaces `ObservableObject`/`@Published`.
- **Persistence:** SwiftData (`@Model`) is the SwiftUI-native default. Core Data remains for legacy interop only.
- **Testing:** Swift Testing (`import Testing`) is the current winning framework for all new unit tests; XCTest is the legacy/adjacent option retained for UI tests, performance tests, and existing suites. **In this repo, that last clause is the whole story — every Swift test is XCTest; write XCTest** (see the repo exception under "Testing with Swift Testing").
- **Formatting:** `swift-format`, bundled in the toolchain since Swift 6.0 (Xcode 16), invoked as `swift format`.
- **Linting:** SwiftLint (community ecosystem linter), current stable 0.65.0 — optional, complements swift-format.
- **App Store floor:** Since April 28, 2026, iOS/iPadOS apps uploaded to App Store Connect must be built with the iOS 26 SDK or later (Xcode 26+).
- **Research date:** July 11, 2026
- **Research basis:** current official docs, release notes, specifications, changelogs, and primary repositories.

## Strict concurrency and the Swift 6 language mode

This is the defining feature of Swift 6. In the Swift 6 language mode, **data-race safety is enforced at compile time**: the compiler must be able to prove that mutable state is never accessed concurrently from two isolation domains. The whole model is built from four concepts — isolation domains, `Sendable`, actors, and region-based `sending` — and getting them right is the core skill.

### Actors and actor isolation

An `actor` is a reference type that serializes all access to its mutable state. Every stored property and method is *isolated* to the actor; from outside, you must `await` to cross the isolation boundary.

```swift
actor ImageCache {
    private var storage: [URL: Data] = [:]

    func data(for url: URL) -> Data? {
        storage[url]              // synchronous inside the actor
    }

    func insert(_ data: Data, for url: URL) {
        storage[url] = data
    }
}

let cache = ImageCache()
// From outside the actor, every access hops the boundary:
await cache.insert(bytes, for: url)
let cached = await cache.data(for: url)
```

**Critical insight:** actor methods called from outside are always `async` at the call site, even when declared synchronously — the `await` marks a potential suspension point where the actor may be busy. Reentrancy is real: an actor can process another message at any `await` inside one of its methods, so never assume state is unchanged across a suspension.

### @MainActor and global actors

A *global actor* isolates declarations spread across your codebase to one shared executor. `@MainActor` is the built-in global actor bound to the main thread — use it for all UI code.

```swift
import SwiftUI

@MainActor
final class ProfileViewModel {
    private(set) var name = ""
    func load() async {
        let fetched = await api.fetchName()   // hops off, then back to main
        name = fetched                        // guaranteed on main thread
    }
}
```

Apply `@MainActor` to a type, method, or property. You can define your own global actor with `@globalActor`, but this is rare — reach for a plain `actor` instance instead unless you genuinely need one shared domain across many types.

### Approachable concurrency: main-actor-by-default (Swift 6.2)

Swift 6.2's biggest ergonomic change is the ability to make `@MainActor` the **default isolation** for a module, so ordinary app code is single-threaded until you explicitly opt into concurrency. New Xcode 26 app projects enable this by default; SPM packages do **not** — you must opt in per target:

```swift
// In Package.swift, tools-version 6.2+
.target(
    name: "AppCore",
    swiftSettings: [
        .defaultIsolation(MainActor.self),          // SE-0466
        .enableUpcomingFeature("NonisolatedNonsendingByDefault"), // SE-0461
        .enableUpcomingFeature("InferIsolatedConformances"),      // SE-0470
    ]
)
```

Two orthogonal settings drive the posture:

- **Default Actor Isolation = `MainActor`** (SE-0466, *Control default actor isolation inference*): unannotated declarations are inferred `@MainActor`. Pass `nil` to `.defaultIsolation` for `nonisolated`.
- **Approachable Concurrency** (`SWIFT_APPROACHABLE_CONCURRENCY` in Xcode): enables `NonisolatedNonsendingByDefault` (SE-0461) and `InferIsolatedConformances` (SE-0470).

With this on, you write UI and script code with no `@MainActor` annotations, and only step off the main actor deliberately. **This is the recommended posture for apps and executables** — it eliminates the vast majority of false-positive concurrency diagnostics. For libraries meant to run work concurrently (networking, utility targets), leave default isolation off and annotate deliberately.

### The three isolation tools you actually reach for

```swift
import Foundation

// 1. Main-actor by default (implicit) — UI and app state live here.
@Observable
final class LibraryModel {
    var books: [Book] = []

    // Runs on the main actor. Fine for orchestration and quick work.
    func addPlaceholder() {
        books.append(Book(title: "Untitled", isbn: UUID().uuidString))
    }

    // 2. @concurrent — OFFLOAD real CPU work to the global executor.
    //    Use ONLY for genuinely expensive work (parsing, hashing, image processing).
    @concurrent
    func indexFullText(_ raw: Data) async throws -> [String: [Int]] {
        try await Task.sleep(for: .milliseconds(1)) // stand-in for heavy work
        return buildInvertedIndex(from: raw)
    }

    // 3. nonisolated — pure, isolation-free helpers with no actor state.
    nonisolated func buildInvertedIndex(from data: Data) -> [String: [Int]] {
        [:] // pure computation, callable from anywhere
    }
}
```

### nonisolated, nonisolated(nonsending), and @concurrent

`nonisolated` opts a declaration *out* of its enclosing isolation — use it for pure, stateless members that touch no isolated state:

```swift
@MainActor
final class Formatter {
    let locale: Locale
    init(locale: Locale) { self.locale = locale }

    nonisolated var identifier: String { locale.identifier }  // no isolated state
}
```

Two Swift 6.2 features refine how async functions pick their executor:

- **`nonisolated(nonsending)`** (SE-0461): an async function runs in the *caller's* execution context instead of hopping to the global concurrent pool. This is enabled by default under approachable concurrency and eliminates a whole class of false-positive `Sendable` errors on async class methods.
- **`@concurrent`** (SE-0461): the explicit opt-in for "run this async function on the concurrent thread pool, off the caller's actor." Use it when you deliberately want parallel background work.

**Critical insight:** under approachable concurrency a plain `nonisolated async` function runs on the *caller's* actor, so calling it from the main actor does **not** free the main thread. Only `@concurrent` guarantees the function hops to the global concurrent executor. Agents that expect "async == background thread" will block the UI — that assumption is wrong on this stack.

```swift
@MainActor
final class ReportBuilder {
    var rows: [Row] = []

    // Runs on the main actor by inheriting the caller (nonisolated(nonsending)).
    func summarize() async -> Summary { Summary(count: rows.count) }

    // Deliberately parallel: hops onto the concurrent pool.
    @concurrent
    func render(_ rows: [Row]) async -> Data { expensivePDF(rows) }
}
```

### Sendable and how the compiler checks it

`Sendable` is a marker protocol meaning "safe to pass across isolation boundaries." Only `Sendable` values may cross between actors/tasks. Value types with `Sendable` members conform *implicitly*; you cannot make a class with mutable stored state `Sendable` unless it is `final` and immutable (all `let`, all `Sendable`).

```swift
struct User: Sendable {           // implicit: all members are Sendable
    let id: UUID
    let name: String
}

final class Config: Sendable {    // OK: final + all immutable Sendable lets
    let baseURL: URL
    init(baseURL: URL) { self.baseURL = baseURL }
}
```

**The single most common Swift 6 mistake is silencing this check with `@unchecked Sendable`.** `@unchecked Sendable` disables the compiler's proof and makes *you* responsible for thread safety — every use is a place a data race can hide. It compiles but can crash at runtime. Reach for it only around a genuine lock/queue you own, and prefer an `actor` or a truly immutable type instead:

```swift
// WRONG — reintroduces the race the compiler just prevented:
final class Counter: @unchecked Sendable {
    var value = 0     // mutable, unprotected
}

// RIGHT — an actor serializes access, no unsafe escape hatch:
actor Counter {
    private(set) var value = 0
    func increment() { value += 1 }
}
```

### sending parameters and region-based isolation

`sending` (SE-0430, Swift 6.0) lets a function accept or return a *non-`Sendable`* value by transferring ownership across an isolation boundary. The compiler proves, via region-based analysis, that the caller no longer uses the value after transfer — so no copy and no race. It is useful for handing a freshly built object to an actor.

```swift
final class Payload {}            // deliberately not Sendable

actor Uploader {
    private var pending: Payload?
    func stage(_ p: sending Payload) {   // ownership transferred in
        pending = p
    }
}

func hand(off uploader: Uploader) async {
    let p = Payload()
    await uploader.stage(p)   // OK: 'p' sent into the actor
    // Using 'p' here would be a compile error — it was sent away.
}
```

Prefer `sending` over `@Sendable`/`@unchecked` when you want to move a value exactly once rather than share it.

### isolated parameters and @Sendable closures

An `isolated` parameter lets a function run *within* a given actor's isolation:

```swift
func addLog(_ message: String, to cache: isolated ImageCache) {
    // Runs inside the actor; no 'await' needed on cache's members here.
}
```

A `@Sendable` closure is one that may be passed across isolation domains; it can only capture `Sendable` values. `Task { }` bodies and `TaskGroup.addTask { }` closures are `@Sendable`, which is why capturing mutable non-`Sendable` state in them is an error — snapshot the value first:

```swift
// WRONG: captures mutable actor-isolated state in a Sendable closure
// RIGHT: snapshot, then send the immutable snapshot
let snapshot = model.currentState        // Sendable value
Task {
    await repository.save(snapshot)
}
```

### async/await and structured concurrency

Use `async`/`await` for all asynchronous work. **Never** wrap it in completion handlers or `DispatchQueue`. Structured concurrency ties child task lifetimes to a scope:

```swift
// async let: a fixed, small number of concurrent operations
func loadDashboard() async throws -> Dashboard {
    async let profile = fetchProfile()
    async let posts = fetchPosts()
    async let notifications = fetchNotifications()
    return try await Dashboard(profile: profile, posts: posts, notifications: notifications)
}
```

For a dynamic number of tasks, use a task group. `withThrowingTaskGroup` propagates errors and cancels remaining children when the body rethrows:

```swift
func loadThumbnails(for ids: [Book.ID]) async throws -> [Book.ID: Image] {
    try await withThrowingTaskGroup(of: (Book.ID, Image).self) { group in
        for id in ids {
            group.addTask { (id, try await fetchThumbnail(id)) }
        }
        var result: [Book.ID: Image] = [:]
        for try await (id, image) in group {   // results arrive as they complete
            result[id] = image
        }
        return result
    }
}
```

Use `withDiscardingTaskGroup` (Swift 5.9+) when you don't need results. Results arrive out of order — track indices/keys if order matters.

### Task, priorities, and cancellation

An unstructured `Task { }` starts concurrent work not tied to a scope (e.g., from a synchronous UI callback). Cancellation is cooperative: check `Task.isCancelled` or call `try Task.checkCancellation()`.

```swift
@MainActor
final class SearchController {
    private var searchTask: Task<Void, Never>?

    func search(_ query: String) {
        searchTask?.cancel()                 // cancel the previous in-flight search
        searchTask = Task {
            do {
                try Task.checkCancellation()
                let results = try await api.search(query)
                guard !Task.isCancelled else { return }
                self.display(results)
            } catch is CancellationError {
                // expected on rapid re-typing
            } catch {
                self.show(error)
            }
        }
    }
}
```

Use `Task.sleep(for:)` with a `Duration` (e.g., `.seconds(1)`), not `sleep()`. Prefer structured concurrency to unstructured `Task`; use `Task.detached` only when you truly must break inheritance of context/priority.

### AsyncSequence, AsyncStream, and continuations

`AsyncSequence` is the async analog of `Sequence`, iterated with `for await`. `AsyncStream` bridges callback/delegate APIs into async iteration:

```swift
func locationUpdates() -> AsyncStream<CLLocation> {
    AsyncStream { continuation in
        let monitor = LocationMonitor()
        monitor.onUpdate = { continuation.yield($0) }
        continuation.onTermination = { _ in monitor.stop() }
        monitor.start()
    }
}

for await location in locationUpdates() {
    update(with: location)
}
```

To bridge a one-shot completion-handler API into `async`, use a checked continuation — it enforces that you resume exactly once:

```swift
func loadImage(named name: String) async throws -> UIImage {
    try await withCheckedThrowingContinuation { continuation in
        legacyLoader.loadImage(name) { image, error in
            if let image {
                continuation.resume(returning: image)
            } else {
                continuation.resume(throwing: error ?? LoadError.unknown)
            }
        }
    }
}
```

Use `withCheckedContinuation`/`withCheckedThrowingContinuation` in development (they trap on double-resume); the `Unsafe` variants only in measured hot paths.

### Task-local values

`@TaskLocal` propagates contextual values (request IDs, trace context) down a task tree without threading them through every signature:

```swift
enum RequestContext {
    @TaskLocal static var traceID: String?
}

RequestContext.$traceID.withValue("abc-123") {
    await handleRequest()   // any child task reads RequestContext.traceID
}
```

## Value types, references, and the type-selection decision

Swift is value-semantics-first. Default to `struct` and `enum`; reach for `class`/`actor` only when you need reference semantics or isolation.

| Need | Use |
|------|-----|
| Data model, DTO, most types | `struct` |
| Fixed set of cases / state machine | `enum` (with associated values) |
| Reference identity, inheritance, or `deinit` | `final class` |
| Shared mutable state across concurrency | `actor` |
| UI-bound observable state | `@Observable final class` (main-actor by default) |
| Unique-ownership resource (file handle, token) | `~Copyable struct` |

Make classes `final` by default; subclassing is opt-in, not the norm.

```swift
enum PaymentState {
    case idle
    case processing(orderID: UUID)
    case completed(receipt: Receipt)
    case failed(Error)
}
```

## Protocols, generics, and the some/any distinction

Protocol-oriented programming is idiomatic: model capabilities as protocols and add default implementations in extensions.

```swift
protocol Repository<Item> {           // primary associated type (Swift 5.7)
    associatedtype Item
    func fetch(id: UUID) async throws -> Item
}

extension Repository {
    func fetchOrNil(id: UUID) async -> Item? { try? await fetch(id: id) }
}
```

**`some` vs `any` is the highest-value generics decision** (both stabilized Swift 5.7):

| Use | When |
|-----|------|
| `some P` (opaque type) | One specific concrete type, fixed at compile time, zero-cost. Prefer this. |
| `any P` (existential) | You genuinely need to store/mix different conforming types at runtime; has boxing cost. |
| `<T: P>` (generic) | The caller chooses the type and you need to relate multiple positions. |

```swift
func makeStore() -> some Repository<User> { UserStore() }     // opaque: one type
var plugins: [any Plugin] = [LogPlugin(), CachePlugin()]      // existential: heterogeneous
func sync<R: Repository>(_ r: R) async { /* generic over R */ }
```

Parameter packs / variadic generics (Swift 5.9) let you write functions generic over an arbitrary number of type parameters:

```swift
func zipAll<each T>(_ item: repeat each T) -> (repeat each T) {
    (repeat each item)
}
```

## Optionals, error handling, and pattern matching

Use `if let`/`guard let` shorthand (Swift 5.7) that omits the repeated name, and `guard` for early exit:

```swift
func greet(_ name: String?) -> String {
    guard let name else { return "Hello, stranger" }   // shorthand binding
    return "Hello, \(name)"
}
```

**Never force-unwrap (`!`) in production paths.** Use `guard let`, `??`, or optional chaining. Force-unwrap only when a `nil` is a genuine programmer error you want to trap loudly, and even then prefer a descriptive `precondition`.

Errors use `throws`/`do`/`catch`. `Result` is for storing or passing an outcome, not for control flow:

```swift
enum ValidationError: Error { case tooShort, invalidCharacter }

func validate(_ s: String) throws {
    guard s.count >= 8 else { throw ValidationError.tooShort }
}

do {
    try validate(password)
} catch ValidationError.tooShort {
    show("Password too short")
} catch {
    show("Unexpected: \(error)")
}
```

### Typed throws (Swift 6.0)

`throws(SomeError)` declares the exact error type a function throws. Per SE-0413, untyped `throws` remains the right default for most code — the proposal states plainly: "Even with the introduction of typed throws into Swift, the existing (untyped) throws remains the better default error-handling mechanism for most Swift code." Use typed throws only in constrained cases: a function with a single closed error domain that stays within a module where you always exhaustively handle the error, generic pass-through code (a better `rethrows`), and dependency-free/embedded code.

```swift
enum CopierError: Error { case outOfPaper }

func copy(pages: Int) throws(CopierError) -> Int {
    guard hasPaper else { throw .outOfPaper }
    return pages
}

do {
    _ = try copy(pages: 3)
} catch {
    // 'error' is statically typed as CopierError — no downcast needed
}
```

`throws(any Error)` is equivalent to plain `throws`; `throws(Never)` is a non-throwing function.

Pattern matching with `switch` is exhaustive and supports `where` clauses, value binding, and tuple matching:

```swift
switch state {
case .completed(let receipt) where receipt.total > 0:
    archive(receipt)
case .completed, .idle:
    break
case .processing(let id):
    log("in flight: \(id)")
case .failed(let error):
    report(error)
}
```

## Noncopyable types and ownership

`~Copyable` (Swift 5.9, generalized to generics in 6.0) suppresses the implicit `Copyable` conformance, guaranteeing a value has **unique ownership** — assignment moves rather than copies. Use it for resources that must not be duplicated (file descriptors, tokens, transactions). Do **not** reach for it on ordinary data models; a plain `struct` is simpler and faster to work with.

```swift
struct FileHandle: ~Copyable {
    private let fd: Int32

    init(path: String) throws {
        fd = open(path, O_RDONLY)
        guard fd >= 0 else { throw FileError.cannotOpen }
    }

    borrowing func peek() -> UInt8 { /* read without consuming */ 0 }
    consuming func close() { Darwin.close(fd) }   // invalidates the value

    deinit { Darwin.close(fd) }   // noncopyable structs may have deinit
}

func use() throws {
    let handle = try FileHandle(path: "/tmp/data")
    _ = handle.peek()   // borrowing: still usable after
    handle.close()      // consuming: 'handle' invalid past this point
}
```

Ownership modifiers on parameters: `borrowing` (read-only temporary access, the default for most parameters), `consuming` (takes ownership, value invalid at caller afterward), and `inout` (temporary write access). For noncopyable parameters you must state one explicitly. Pattern matching can borrow noncopyable enums when switching (SE-0432, Swift 6.0).

## Memory model: ARC, value semantics, and reference cycles

Value types have **copy-on-write** for their standard-library containers (`Array`, `Dictionary`, `Set`, `String`): copies are cheap until mutated. Reference types (`class`, `actor`) are managed by ARC.

The one memory pitfall to actively prevent is **retain cycles** in closures and delegate-like references. Use a `[weak self]` capture list in escaping closures that a reference type stores:

```swift
final class DownloadController {
    var onFinish: (() -> Void)?

    func start() {
        service.fetch { [weak self] data in
            guard let self else { return }
            self.handle(data)
        }
    }
}
```

| Capture | When |
|---------|------|
| `[weak self]` | `self` may outlive the closure, or the closure is stored long-term. Default choice for escaping stored closures. |
| `[unowned self]` | `self` is *guaranteed* to outlive the closure and you want to avoid the optional. Traps if wrong. |
| strong (default) | Short-lived, non-stored closures (e.g., `map`, most structured-concurrency task bodies). |

Prefer `weak` over `unowned` unless you can prove the lifetime relationship.

## Standard library essentials

### Collections and String

`Array`, `Dictionary`, and `Set` are value types with COW. Prefer functional transforms (`map`, `filter`, `reduce`, `compactMap`) and lazy chains for large sequences. `String` is **Unicode-correct**: it is a collection of `Character` (grapheme clusters), not code units. Index into it via `String.Index`, and use the appropriate view (`.unicodeScalars`, `.utf8`, `.utf16`) when you need lower-level access.

```swift
let flag = "👨‍👩‍👧‍👦"
flag.count                // 1 — one grapheme cluster
flag.unicodeScalars.count // 7 — several scalars
```

`InlineArray<N, Element>` (Swift 6.2) is a fixed-size, stack-allocated array with no heap allocation — for performance-critical, fixed-count data. Shorthand: `[3 of Float]`. `Span<Element>` (Swift 6.2) is a safe, non-owning, bounds-checked view into contiguous memory, replacing most uses of unsafe buffer pointers; it is non-escapable and lifetime-checked at compile time.

```swift
struct RGBA { var channels: [4 of UInt8] }        // InlineArray, on the stack

func sum(_ span: Span<Int>) -> Int {              // safe view, no copy
    var total = 0
    for i in span.indices { total += span[i] }
    return total
}
let numbers = [1, 2, 3, 4]
let total = sum(numbers.span)
```

### Codable and serialization

Conform to `Codable` for JSON and other formats. Use `CodingKeys` to remap names and `JSONDecoder` strategies rather than hand-writing decoders. Keep decoded network models (DTOs) as `Sendable` structs, separate from your `@Model`/`@Observable` types:

```swift
struct Article: Codable, Sendable {
    let id: UUID
    let title: String
    let publishedAt: Date

    enum CodingKeys: String, CodingKey {
        case id
        case title
        case publishedAt = "published_at"
    }
}

let decoder = JSONDecoder()
decoder.keyDecodingStrategy = .convertFromSnakeCase   // alternative to explicit keys
decoder.dateDecodingStrategy = .iso8601
let article = try decoder.decode(Article.self, from: data)
```

Write a manual `init(from:)` only when the payload genuinely doesn't map to your model.

### Foundation vs swift-foundation

A Swift-native reimplementation of Foundation (`swift-foundation`) now backs core types across platforms, bringing the same `Date`, `Data`, `JSONEncoder`, `FormatStyle`, `Predicate`, and calendar APIs to macOS, iOS, Linux, and Windows. On size-sensitive server/CLI targets, import `FoundationEssentials` to get the core subset without internationalization data. Just `import Foundation` for app code. Use `URL.documentsDirectory`, `Duration`/`.seconds`, `Clock`, and `FormatStyle` (`value.formatted(.currency(code: "USD"))`) for modern Foundation work.

### Regex and RegexBuilder

Swift has first-class regex (Swift 5.7). Regex literals `/.../` are checked at **compile time** and produce strongly typed captures. For complex patterns, `RegexBuilder` is a readable DSL:

```swift
import RegexBuilder

let dateRegex = Regex {
    TryCapture { Repeat(.digit, count: 4) } transform: { Int($0) }
    "-"
    TryCapture { Repeat(.digit, count: 2) } transform: { Int($0) }
    "-"
    TryCapture { Repeat(.digit, count: 2) } transform: { Int($0) }
}

if let match = "2026-03-15".wholeMatch(of: dateRegex) {
    let (_, year, month, day) = match.output   // typed as Int
    print(year, month, day)
}
```

Prefer regex literals for simple patterns; `RegexBuilder` when readability and typed transforms matter. Use Foundation's date/number parsers rather than hand-rolling regex for those.

## Macros

Swift macros (Swift 5.9) generate code at compile time; they are type-checked, hygienic, and additive (they never delete your code). You *use* many without authoring any — `@Observable`, `@Model`, `@Test`, `#expect`, `#Predicate`.

There are two families: **freestanding** (`#name`, expression or declaration) and **attached** (`@Name`, attached to a declaration with roles like `peer`, `member`, `accessor`, `memberAttribute`, `extension`). Declare a macro with an `#externalMacro` implementation and author it in a separate macro target that depends on `swift-syntax`:

```swift
// Declaration (in your library)
@freestanding(expression)
public macro URL(_ string: String) -> URL =
    #externalMacro(module: "MyMacros", type: "URLMacro")

// Usage — validated at compile time
let endpoint = #URL("https://example.com")
```

Swift 6.3 adds prebuilt swift-syntax binaries for shared macro libraries, cutting the notorious macro build-time cost. Author a macro only when a protocol default, generic, or property wrapper cannot express the pattern — macros are powerful but raise build cost and complexity.

## The Observation framework

`@Observable` (from the `Observation` module, Swift 5.9; iOS 17/macOS 14+) is the current model layer for SwiftUI. **It replaces the entire `ObservableObject` + `@Published` + `@StateObject`/`@ObservedObject`/`@EnvironmentObject` complex** — do not reach for those in new code. SwiftUI tracks which *individual properties* a view's `body` reads and re-renders only when exactly those change — giving fine-grained updates instead of `ObservableObject`'s blanket "any change re-renders everything."

```swift
import SwiftUI

@Observable
final class SearchModel {
    var query: String = ""
    var results: [SearchResult] = []
    @ObservationIgnored var lastFetchedAt: Date?  // excluded from tracking

    var isEmpty: Bool { results.isEmpty }         // computed props tracked via their inputs
}
```

- `@Observable` synthesizes conformance to the `Observable` protocol; every stored property is observed automatically (no `@Published`).
- `@ObservationIgnored` opts a stored property out of tracking (caches, bookkeeping).
- Computed properties are tracked through whatever stored properties they read.

### The three wrappers — decision table

| Situation | Use | Notes |
|---|---|---|
| The view *owns* / creates the model | `@State` | Instantiate once: `@State private var model = SearchModel()` |
| The view *receives* a model and needs `$bindings` | `@Bindable` | `@Bindable var model: SearchModel` |
| The view receives a model, read-only | plain `let`/`var` property | No wrapper needed |
| The model is shared app-wide | `@Environment` | Inject with `.environment(model)` |

```swift
// Owner creates and injects.
@main
struct BookshelfApp: App {
    @State private var library = LibraryModel()
    var body: some Scene {
        WindowGroup {
            RootView().environment(library)
        }
    }
}

// Child receives from environment.
struct RootView: View {
    @Environment(LibraryModel.self) private var library
    var body: some View {
        List(library.books) { Text($0.title) }
    }
}

// Binding to a model you own with @State works directly.
struct SearchView: View {
    @State private var model = SearchModel()
    var body: some View {
        TextField("Search", text: $model.query)   // $ works because of @State
    }
}

// Binding to a model from the environment requires a local @Bindable.
struct FilterView: View {
    @Environment(SearchModel.self) private var model
    var body: some View {
        @Bindable var model = model                // re-wrap for $ bindings
        TextField("Search", text: $model.query)
    }
}
```

**Critical insight:** you cannot form a `$binding` to a model obtained from `@Environment` directly. Introduce `@Bindable var model = model` at the top of `body`. This is the single most common Observation mistake.

**Behavioral gotcha:** with `@Observable` you use `@State` for ownership (replacing the `init` autoclosure behavior of `@StateObject`). Because `@State` re-runs the initializer expression on each view rebuild, do not perform expensive or side-effecting work in the model's `init` at the view's `@State` site the way you might have with `@StateObject`.

Combine's `@Published`/`ObservableObject` is effectively superseded for UI state; use `@Observable` plus `AsyncSequence` for streams.

### Streaming observation outside SwiftUI (iOS 26 / Swift 6.2)

`Observations` is an `AsyncSequence` that emits new values whenever any property you touch in its closure changes. Updates are transactional — multiple synchronous mutations coalesce into one emitted value. It has *didSet* semantics (you see values after they are assigned).

```swift
let model = SearchModel()

Task { [weak model] in
    guard let model else { return }
    let stream = Observations { model.query }     // weak-capture to avoid retain cycles
    for await query in stream {
        await runSearch(query)
    }
}
```

Use `Observations` (not `withObservationTracking`, which is single-shot and will-set only) for driving persistence, logging, or non-SwiftUI reactions. To end iteration, make the observed value optional and set it to `nil`. Requires OS 26; it is not back-deployed.

## SwiftUI app, state, and navigation architecture

### App and scene structure

```swift
import SwiftUI
import SwiftData

@main
struct BookshelfApp: App {
    var body: some Scene {
        WindowGroup {
            RootView()
        }
        .modelContainer(for: Book.self)   // SwiftData stack for the whole hierarchy
    }
}
```

### Value-based navigation with NavigationStack

Use `NavigationStack` with a typed route enum and `navigationDestination(for:)`. Never use `NavigationView` or `NavigationLink(destination:)` push style — both are superseded.

```swift
enum Route: Hashable {
    case bookDetail(Book.ID)
    case authorDetail(Author.ID)
    case settings
}

@Observable
final class Router {
    var path: [Route] = []
    func push(_ route: Route) { path.append(route) }
    func popToRoot() { path.removeAll() }
}

struct RootView: View {
    @State private var router = Router()
    @Environment(LibraryModel.self) private var library

    var body: some View {
        @Bindable var router = router
        NavigationStack(path: $router.path) {
            List(library.books) { book in
                NavigationLink(book.title, value: Route.bookDetail(book.id))
            }
            .navigationTitle("Library")
            .navigationDestination(for: Route.self) { route in
                switch route {
                case .bookDetail(let id):   BookDetailView(id: id)
                case .authorDetail(let id): AuthorDetailView(id: id)
                case .settings:             SettingsView()
                }
            }
        }
    }
}
```

- Use `[Route]` (a homogeneous typed array) when a single type drives the stack; use `NavigationPath` when the stack mixes heterogeneous value types.
- `NavigationSplitView` for multi-column iPad/Mac layouts.
- Centralize the path in an `@Observable` Router injected via environment for deep links and programmatic control.

### State property wrappers — when to use which

| Wrapper | Purpose |
|---|---|
| `@State` | View-owned value or owned `@Observable` model |
| `@Binding` | Two-way reference to state owned elsewhere |
| `@Bindable` | Bindings to an `@Observable` you don't own |
| `@Environment` | Read injected `@Observable` objects or environment values |
| `@FocusState` | Keyboard/field focus |
| `@AppStorage` / `@SceneStorage` | UserDefaults-backed / per-scene restoration state |

### Custom environment values with @Entry (iOS 18)

```swift
extension EnvironmentValues {
    @Entry var cardStyle: CardStyle = .standard   // no manual EnvironmentKey boilerplate
}

// Set and read:
ContentView().environment(\.cardStyle, .compact)

struct ContentView: View {
    @Environment(\.cardStyle) private var cardStyle
    var body: some View { /* ... */ }
}
```

`@Entry` replaces the old `EnvironmentKey` + computed-property boilerplate and works back to older OS versions when built with Xcode 16+. It also works for `FocusedValues`, `ContainerValues`, and `Transaction`.

### Modern layout, lists, and scrolling

```swift
struct GalleryView: View {
    let items: [Item]
    @State private var scrolledID: Item.ID?

    var body: some View {
        ScrollView(.horizontal) {
            LazyHStack(spacing: 16) {
                ForEach(items) { item in
                    CardView(item: item)
                        .containerRelativeFrame(.horizontal)   // iOS 17+
                }
            }
            .scrollTargetLayout()
        }
        .scrollTargetBehavior(.viewAligned)      // snap to views
        .scrollPosition(id: $scrolledID)         // read/drive position (iOS 17+)
    }
}
```

Use `Grid`/`GridRow` for aligned 2-D layouts, `LazyVGrid`/`LazyHGrid` for large scrollable collections, `ViewThatFits` for adaptive layouts, and a custom `Layout` conformance only when the built-ins cannot express the geometry. Prefer `List` with `ForEach` over `ScrollView`+`VStack` when you need selection, swipe actions, or huge datasets.

### Animations

```swift
// Implicit, value-driven (always pass a value):
.animation(.snappy, value: isExpanded)

// Phase animator — cycles through discrete phases.
Image(systemName: "bell.fill")
    .phaseAnimator([1.0, 1.3, 1.0], trigger: rings) { view, scale in
        view.scaleEffect(scale)
    }

// Zoom / hero transition (iOS 18):
NavigationLink(value: Route.bookDetail(book.id)) {
    CoverView(book: book)
        .matchedTransitionSource(id: book.id, in: namespace)
}
// On the destination:
.navigationTransition(.zoom(sourceID: book.id, in: namespace))
```

Use `KeyframeAnimator` for timeline-based multi-property choreography, `phaseAnimator` for discrete phase cycles, and `matchedTransitionSource` + `.navigationTransition(.zoom:)` for hero transitions (not `matchedGeometryEffect`, which does not work across `NavigationStack`). Never mix the two spring-parameter APIs (`response`/`dampingFraction` vs `duration`/`bounce`).

### SF Symbols and symbol effects

```swift
Image(systemName: "wifi")
    .symbolEffect(.variableColor.iterative, isActive: isConnecting)
    .symbolRenderingMode(.palette)
    .foregroundStyle(.blue, .gray)
```

### Presentation and empty states

```swift
.sheet(isPresented: $showEditor) {
    EditorView()
        .presentationDetents([.medium, .large])
        .presentationBackground(.thinMaterial)
}

// ContentUnavailableView for empty/error states (iOS 17+):
if library.books.isEmpty {
    ContentUnavailableView("No Books", systemImage: "books.vertical",
                           description: Text("Add a book to get started."))
}
```

Also stable and idiomatic: `.searchable`, `.refreshable`, `.toolbar`/`ToolbarItem`, `.inspector` (iOS 17+), `.alert`/`.confirmationDialog`, `Menu`, custom `ButtonStyle`/`LabelStyle`/`ViewModifier`, `Gauge`, and `@ScaledMetric` for Dynamic Type.

### Previews with #Preview and @Previewable

```swift
#Preview {
    @Previewable @State var query = "swift"
    SearchBar(text: $query)   // @Previewable lets a preview hold live state
}
```

### Concurrency + SwiftUI integration

Views are `@MainActor`-isolated. `@Observable` models under default main-actor isolation are also main-actor, so reading/writing their properties from `body` and from `.task` is safe with no hops.

```swift
struct BookDetailView: View {
    let id: Book.ID
    @Environment(LibraryModel.self) private var library
    @State private var detail: BookDetail?

    var body: some View {
        Group {
            if let detail { DetailContent(detail) }
            else { ProgressView() }
        }
        .task(id: id) {                       // re-runs when id changes; auto-cancels on disappear
            detail = try? await library.loadDetail(id)
        }
    }
}
```

- `.task` and `.task(id:)` inherit main-actor isolation and are automatically cancelled when the view disappears (or when `id` changes). Prefer them over `onAppear` + manual `Task`.
- Do expensive work in a model method marked `@concurrent`; assign the result back to a main-actor `@Observable` property. Because the model is main-actor-isolated, the assignment lands on the main thread automatically — no `MainActor.run` or `DispatchQueue.main` needed.
- Reach for `MainActor.run` only when hopping back from a genuinely `nonisolated`/detached context.

## Data and persistence: SwiftData

SwiftData is the SwiftUI-native persistence layer — use it for new apps. Core Data is the older paired option; only touch it for existing Core Data stores or features SwiftData lacks.

### Model definition

```swift
import SwiftData

@Model
final class Book {
    #Unique<Book>([\.isbn])              // compound uniqueness (iOS 18)
    #Index<Book>([\.title], [\.isbn])    // query indexes (iOS 18)

    var title: String
    @Attribute(.unique) var isbn: String
    var addedAt: Date
    @Relationship(deleteRule: .cascade, inverse: \Review.book) var reviews: [Review] = []

    init(title: String, isbn: String, addedAt: Date = .now) {
        self.title = title
        self.isbn = isbn
        self.addedAt = addedAt
    }
}
```

### Container, queries, and mutation in views

```swift
struct BookListView: View {
    @Query(sort: \Book.addedAt, order: .reverse) private var books: [Book]
    @Environment(\.modelContext) private var context

    var body: some View {
        List {
            ForEach(books) { book in
                Text(book.title)
            }
            .onDelete { indexSet in
                for i in indexSet { context.delete(books[i]) }
            }
        }
        .toolbar {
            Button("Add") {
                context.insert(Book(title: "New Book", isbn: UUID().uuidString))
                // No explicit save() needed — SwiftData autosaves on UI events.
            }
        }
    }
}
```

`@Query` reads from the main-actor `modelContext`, auto-updates the view on changes, and accepts `#Predicate` filters and `SortDescriptor`s.

```swift
@Query(filter: #Predicate<Book> { $0.title.contains("Swift") },
       sort: \Book.title) private var swiftBooks: [Book]
```

### Background work with ModelActor

`@Query` and the main `modelContext` are `@MainActor`-bound. For imports or heavy writes, use a `@ModelActor` so database work runs off the main thread.

```swift
@ModelActor
actor ImportActor {
    func importBooks(_ dtos: [BookDTO]) throws {
        for dto in dtos {
            modelContext.insert(Book(title: dto.title, isbn: dto.isbn))
        }
        try modelContext.save()   // explicit save on background contexts
    }
}

// Kick off from a view, passing the container across the boundary:
.task {
    let actor = ImportActor(modelContainer: context.container)
    try? await actor.importBooks(downloaded)
}
```

**Critical insight:** `PersistentModel` instances are bound to their originating context and are not `Sendable`. Never pass a fetched `@Model` object between actors — pass its `PersistentIdentifier`, or map to a plain `Sendable` struct before crossing the boundary.

### Versioned schemas and migration

```swift
enum SchemaV2: VersionedSchema {
    static var versionIdentifier = Schema.Version(2, 0, 0)
    static var models: [any PersistentModel.Type] { [Book.self] }
    // ... @Model definitions for this version
}

enum BookMigrationPlan: SchemaMigrationPlan {
    static var schemas: [any VersionedSchema.Type] { [SchemaV1.self, SchemaV2.self] }
    static var stages: [MigrationStage] { [migrateV1toV2] }
    static let migrateV1toV2 = MigrationStage.lightweight(
        fromVersion: SchemaV1.self, toVersion: SchemaV2.self)
}
```

Use `.lightweight` for additive/rename changes; `.custom(willMigrate:didMigrate:)` for de-duplication or type transforms. `#Unique` and `#Index` are stable as of iOS 18; schema inheritance/subclassing arrived in iOS 26.

## Networking with async/await URLSession

Use `URLSession`'s async APIs. Do not use completion handlers, and do not use Combine's `dataTaskPublisher` for new SwiftUI code — Combine is legacy for this stack.

```swift
struct BookService: Sendable {
    var session: URLSession = .shared

    func fetchBook(id: Book.ID) async throws -> BookDTO {
        var request = URLRequest(url: URL(string: "https://api.example.com/books/\(id)")!)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
            throw URLError(.badServerResponse)
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(BookDTO.self, from: data)
    }

    // Parallel fetches with structured concurrency.
    func fetchBooks(ids: [Book.ID]) async throws -> [BookDTO] {
        try await withThrowingTaskGroup(of: BookDTO.self) { group in
            for id in ids { group.addTask { try await fetchBook(id: id) } }
            return try await group.reduce(into: []) { $0.append($1) }
        }
    }
}
```

- Decode into `Sendable` `Codable` structs (DTOs), keeping network models separate from your `@Model`/`@Observable` types.
- Use `session.bytes(for:)` for streaming responses.
- Cancellation is automatic: when the enclosing `.task` is cancelled, the in-flight `await` throws `CancellationError`/`URLError(.cancelled)`.

## Testing with Swift Testing

Swift Testing (`import Testing`, Swift 6.0, developed openly on GitHub and discussed on the Swift Forums) is the **current winning framework** — use it for all new tests. It uses macros (`@Test`, `@Suite`, `#expect`, `#require`), runs tests in parallel by default (per Apple: "all tests integrate seamlessly with Swift Concurrency and run in parallel by default"), and replaces XCTest's assertion zoo. **XCTest is the older/adjacent option**, retained for UI tests (XCUITest has no Swift Testing equivalent), performance tests, and existing suites; the two run side by side.

> **Repo exception — Spidola uses XCTest.** Every Swift test in this tree is XCTest (19 files, no
> `import Testing`), and CI's tvOS lane runs them. New tests here follow the tree, not this
> section: `import XCTest`, not `import Testing`. The whole suite is an "existing suite" in the
> sense above, so this is that clause rather than a disagreement with it. Migrating is a
> deliberate, separate decision — not something to start midway through an unrelated change.

```swift
import Testing
@testable import Bookshelf

@Suite("Book parsing")
struct BookParsingTests {
    @Test("Parses a well-formed record")
    func parsesValidRecord() throws {
        let record = try parse(validData)
        #expect(record.title == "Swift")
    }

    // #require stops the test if the value is nil/false (like a guard) and unwraps optionals.
    @Test func requiresNonNil() throws {
        let book = try #require(findBook(isbn: "123"))
        #expect(book.title.isEmpty == false)
    }

    // Parameterized — runs once per argument, in parallel, each independently reportable.
    @Test(arguments: ["", "  ", "\n"])
    func rejectsBlankTitles(_ input: String) {
        #expect(BookValidator.isValidTitle(input) == false)
    }

    // zip to pair inputs with expected outputs (no cartesian product).
    @Test(arguments: zip(["a@b.com", "nope"], [true, false]))
    func validatesEmail(_ email: String, _ expected: Bool) {
        #expect(EmailValidator.isValid(email) == expected)
    }
}

@Suite("Payment flow", .tags(.critical))
struct PaymentTests {
    @Test(.enabled(if: AppFeatures.paymentsOn))
    func charges() async throws { /* ... */ }
}
```

- Use `#expect` for soft assertions (test continues) and `#require` for preconditions (test stops).
- Pass multiple collections to `arguments:` for a Cartesian product, or `zip(...)` them for paired runs.
- Async tests: just mark the test `async` and `await` directly.
- Callback-based code: use `await confirmation { confirm in ... }`.
- Known failures: wrap in `withKnownIssue { ... }`.
- Traits: `.tags(.critical)`, `.timeLimit(.minutes(1))`, `.bug("...")`, `.enabled(if:)`, `.disabled("...")`, and `.serialized` on a `@Suite` to opt out of parallelism.
- Setup/teardown: use the suite type's `init`/`deinit` (a fresh suite instance per test isolates state).
- Swift 6.3 additions: `Issue.record(..., severity: .warning)` records a non-failing warning, and `try Test.cancel()` cancels a running test (or a single parameterized argument).
- Testing `@Observable` models is straightforward since they are plain classes — construct, mutate, assert. For main-actor-isolated types, mark the test or suite `@MainActor`.
- Run from the CLI with `swift test`.

## Tooling and project configuration

### Package.swift

SwiftPM is the standard. A current, copy-ready manifest for an app-core library plus tests:

```swift
// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Bookshelf",
    platforms: [.iOS(.v18), .macOS(.v15)],
    products: [
        .library(name: "BookshelfCore", targets: ["BookshelfCore"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-log.git", from: "1.6.0"),
    ],
    targets: [
        .target(
            name: "BookshelfCore",
            dependencies: [
                .product(name: "Logging", package: "swift-log"),
            ],
            resources: [
                .process("Resources"),
            ],
            swiftSettings: [
                .swiftLanguageMode(.v6),
                .defaultIsolation(MainActor.self),   // tools-version 6.2+ (SE-0466)
                .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
                .enableUpcomingFeature("InferIsolatedConformances"),
            ]
        ),
        .testTarget(
            name: "BookshelfCoreTests",
            dependencies: ["BookshelfCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
```

Key points:

- `.swiftLanguageMode(.v6)` / `swiftLanguageModes: [.v6]` selects the language mode (the older `swiftLanguageVersions` is deprecated, renamed to `swiftLanguageModes` in tools-version 6.0).
- `.defaultIsolation(MainActor.self)` (tools-version 6.2+) makes the target main-actor-by-default; pass `nil` for `nonisolated`. New SPM packages do **not** enable this automatically — set it explicitly for UI/app-logic targets, leave networking/utility targets `nonisolated`.
- Products are `.library`/`.executable`; targets can carry `resources`, `plugins`, and per-target `swiftSettings`. Version dependencies with `from:` (up-to-next-major) as the default.
- Package traits are discoverable via `swift package show-traits` (Swift 6.3). Swift 6.3 also ships a preview of an integrated Swift Build engine for a unified cross-platform build.
- Commands: `swift build`, `swift test`, `swift run`, `swift package resolve`, `swift package update`.

### Macro target (when authoring macros)

```swift
import CompilerPluginSupport
// ...
.macro(
    name: "BookshelfMacros",
    dependencies: [
        .product(name: "SwiftSyntaxMacros", package: "swift-syntax"),
        .product(name: "SwiftCompilerPlugin", package: "swift-syntax")
    ]
)
```

### swift-format (bundled formatter)

`swift-format` is Apple's official formatter, **bundled in the toolchain since Swift 6.0 (Xcode 16, Sept 17, 2024)**. Invoke it as `swift format` (note the space), or find it with `xcrun --find swift-format` — no install needed. Commands: `swift format` (format; the default subcommand), `swift format lint` (report violations), and `swift-format dump-configuration` (emit the default `.swift-format` JSON as a starting point, e.g. `swift-format dump-configuration > .swift-format`). A real `.swift-format` at the repo root:

```json
{
  "version": 1,
  "lineLength": 100,
  "indentation": { "spaces": 4 },
  "maximumBlankLines": 1,
  "respectsExistingLineBreaks": true,
  "lineBreakBeforeControlFlowKeywords": false,
  "lineBreakBeforeEachArgument": false,
  "indentConditionalCompilationBlocks": false,
  "prioritizeKeepingFunctionOutputTogether": true,
  "rules": {
    "AllPublicDeclarationsHaveDocumentation": false,
    "AlwaysUseLowerCamelCase": true,
    "NeverUseImplicitlyUnwrappedOptionals": true,
    "OrderedImports": true,
    "UseLetInEveryBoundCaseVariable": true,
    "ReturnVoidInsteadOfEmptyTuple": true
  }
}
```

Run recursively in place, and fail CI on violations:

```bash
swift format --in-place --recursive Sources Tests
swift format lint --strict --recursive Sources Tests
```

The config `version` key is currently `1`.

### SwiftLint (ecosystem linter)

SwiftLint (community-maintained, current stable 0.65.0) enforces conventions beyond formatting; install via Homebrew (`brew install swiftlint`) or the SwiftPM plugin, and run `swiftlint` / `swiftlint lint` / `swiftlint --fix`. It is distinct from swift-format (different governance, different `.swiftlint.yml` config) and optional on top of it. A real `.swiftlint.yml`:

```yaml
disabled_rules:
  - trailing_whitespace
opt_in_rules:
  - empty_count
  - empty_string
  - force_unwrapping
  - explicit_init
analyzer_rules:
  - unused_import
included:
  - Sources
  - Tests
excluded:
  - .build
  - Sources/Generated
line_length:
  warning: 120
  error: 200
  ignores_urls: true
  ignores_comments: true
force_cast: error
force_try: error
identifier_name:
  min_length: 2
  excluded: [id, x, y, z]
reporter: "xcode"
```

### Strict-concurrency compiler flags (migration only)

When incrementally hardening a target that isn't yet in Swift 6 mode, enable checking via `swiftSettings`:

```swift
swiftSettings: [
    .enableExperimentalFeature("StrictConcurrency"),  // "complete" checking
    .enableUpcomingFeature("InferIsolatedConformances"),
]
```

New code should simply target the Swift 6 language mode, where complete checking is the baseline.

## Logging and cross-platform concerns

### Logging

On Apple platforms use **`Logger` from OSLog** (`import OSLog`) — Apple's recommended replacement for `print`/`NSLog`, with levels, subsystems, categories, privacy redaction, and near-zero overhead. Never ship `print` for diagnostics.

```swift
import OSLog

extension Logger {
    private static let subsystem = Bundle.main.bundleIdentifier!
    static let networking = Logger(subsystem: subsystem, category: "networking")
}

Logger.networking.info("Request started: \(url, privacy: .public)")
Logger.networking.error("Failed: \(error.localizedDescription)")
```

For cross-platform code (server, CLI, Linux) use **swift-log** (`import Logging`), the SSWG-endorsed logging API; bootstrap it once at startup and swap backends without touching call sites:

```swift
import Logging

let logger = Logger(label: "com.example.service")
logger.info("Service started", metadata: ["port": "8080"])
```

### Cross-platform and interop

Swift 6.3 ships the first official release of the **Swift SDK for Android** — described by the Swift team as "a significant milestone that opens new opportunities for cross-platform development in Swift" — alongside mature Linux and Windows support. **Embedded Swift** (a language subset producing small, standalone binaries for microcontrollers; ARM/RISC-V) is still evolving but gained full `String` APIs and `InlineArray`/`Span` support in 6.2 — treat it as advanced, non-default territory. WebAssembly compilation is supported via the Wasm SDK (Swift 6.2).

For **C++ interoperability**, enable it per target with `.interoperabilityMode(.Cxx)` in `swiftSettings`; SwiftPM auto-generates the module map from an umbrella header. C++ types with no copy constructor import as `~Copyable`. Enabling C++ interop is a breaking change for a package's clients, so bump the major version when you add it. Plain C and Objective-C interop is on by default and needs no flag.

```swift
.target(
    name: "AppCore",
    dependencies: ["CxxEngine"],
    swiftSettings: [.interoperabilityMode(.Cxx)]
)
```

On the server, the current ecosystem centers on async/await-native frameworks and the SSWG packages (swift-log, swift-nio, swift-metrics); adopt async APIs throughout rather than any callback-based holdovers.

## Anti-patterns to avoid

| ❌ Wrong (superseded / adjacent-ecosystem habit) | ✅ Right (this stack) |
|---|---|
| `@unchecked Sendable` to silence the compiler | `actor`, truly immutable `final class`, or `sending` — the escape hatch disables the data-race proof and can crash at runtime |
| `DispatchQueue.main.async { model.x = ... }` / GCD for app logic | Main-actor `@Observable` property — assign directly; `@MainActor`, `Task`, `@concurrent`, structured concurrency |
| Completion handlers for new async APIs | `async`/`await`; bridge unavoidable legacy callbacks with `withCheckedContinuation` (resume exactly once) |
| Assuming `nonisolated async` runs off the main thread | Use `@concurrent` to actually offload work |
| Assuming actor state is stable across `await` | Actors are reentrant — re-check invariants after every suspension point |
| Capturing mutable, non-`Sendable` state in a `Task`/`addTask` closure | Snapshot an immutable `Sendable` value first, or transfer with `sending` |
| Blocking inside `async` code (`sleep`, semaphores, sync file I/O on the main actor) | `Task.sleep(for:)` and async I/O |
| Force-unwrapping (`!`) and force-try (`try!`) | `guard let`, `??`, optional chaining, `do`/`catch`; reserve `!` for genuine invariants and prefer `precondition` |
| `class` by default (models, view models, test suites) | `struct`/`enum` and `@Observable`; `final class`/`actor` only for identity or shared isolation; test suites default to `struct` |
| Strong `self` in stored escaping closures (retain cycles) | `[weak self]` + `guard let self else { return }` |
| Overusing `any` existentials | `some`/generics for zero-cost abstraction; `any` only for genuinely heterogeneous storage |
| `~Copyable` on ordinary models | Plain `struct`; ownership ceremony only when the value must be unique |
| Hand-writing `Codable` conformances | `CodingKeys` plus a decoder strategy |
| `class VM: ObservableObject { @Published var x }` | `@Observable final class VM { var x }` |
| `@StateObject` / `@ObservedObject` / `@EnvironmentObject` | `@State` / `@Bindable` / `@Environment` |
| `@Observable class VM: ObservableObject { @Published ... }` | `@Observable` alone — never mix; `@Published` is inert with `@Observable` |
| `$binding` on an `@Environment` model directly | `@Bindable var model = model` in `body`, then `$model.x` |
| `NavigationView` + `NavigationLink(destination:)` | `NavigationStack` + `NavigationLink(value:)` + `navigationDestination(for:)` |
| Combine `dataTaskPublisher` / `.sink` for networking | `try await URLSession.shared.data(for:)` |
| `URLSession.dataTask(with:) { data, _, _ in }` | `let (data, response) = try await session.data(for: request)` |
| Passing a fetched `@Model` between actors | Pass `PersistentIdentifier`, or map to a `Sendable` DTO |
| XCTest for new unit tests | Swift Testing `@Test` / `#expect` / `#require` |
| `withObservationTracking` for ongoing observation | `Observations { ... }` async sequence (OS 26) |
| `matchedGeometryEffect` across a `NavigationStack` | `matchedTransitionSource` + `.navigationTransition(.zoom:)` |

## Quick reference

| Concern | Modern choice (floor) |
|---------|----------------------|
| Concurrency safety | Swift 6 language mode, static data-race checking |
| App isolation posture | main-actor-by-default via `.defaultIsolation(MainActor.self)` (6.2) |
| Shared mutable state | `actor` (never `@unchecked Sendable`) |
| Background work | `@concurrent` async funcs / TaskGroup (6.2) |
| Cross-boundary move | `sending` parameters (6.0 / SE-0430) |
| UI observable state | `@Observable` + `@State`/`@Bindable`/`@Environment` (5.9; iOS 17+) |
| Non-SwiftUI observation | `Observations` async sequence (iOS 26 / Swift 6.2) |
| Navigation | `NavigationStack` + typed routes + `navigationDestination(for:)` |
| Persistence | SwiftData `@Model` (+ `#Unique`/`#Index` iOS 18; `@ModelActor` for background) |
| Networking | async/await `URLSession` (`data(for:)`, `bytes(for:)`) |
| Async streams | `AsyncStream` / `AsyncSequence` (not Combine) |
| Existential vs opaque | `some` by default, `any` only when heterogeneous (5.7) |
| Error typing | plain `throws`; `throws(E)` only in constrained cases (6.0) |
| Unique resources | `~Copyable` + `consuming`/`borrowing` (5.9 / 6.0) |
| Fixed-size buffers | `InlineArray` / `Span` (6.2) |
| Tests | Swift Testing `@Test`/`#expect` (6.0); XCTest only for UI/perf/legacy |
| Formatting | `swift format` (bundled since 6.0 / Xcode 16) |
| Linting | SwiftLint 0.65.0 (optional) |
| Logging | OSLog `Logger` (Apple) / swift-log (server) |
| Packaging | SwiftPM, tools-version 6.2+ |
| Regex | literals `/.../` or `RegexBuilder` (5.7) |
| Macros | use built-ins; author with swift-syntax only when needed (5.9) |

## Version & compatibility

| Component | Current stable | Notes |
|---|---|---|
| Swift | 6.3 (patch 6.3.2 in Xcode 26.5) | Ships in Xcode 26.4; 6.4 is beta only (Xcode 27) |
| Xcode | 26.4 (Swift 6.3, build 17E192) / 26.5 (Swift 6.3.2) | Compiler version is fixed per Xcode; language mode is separate |
| Language mode | Swift 6 (`.swiftLanguageMode(.v6)`) | Strict concurrency on by default |
| Observation | `@Observable`, `@Bindable` (iOS 17+); `Observations` async sequence (iOS 26 / Swift 6.2) | |
| SwiftData | `#Unique`, `#Index` (iOS 18); schema inheritance (iOS 26) | |
| Swift Testing | Bundled with toolchain (Swift 6.0+) | Default for new unit tests |
| swift-format | Bundled since Xcode 16 (Sept 17, 2024) | Apple-official formatter |
| SwiftLint | 0.65.0 | Community ecosystem linter |
| App Store | Xcode 26 + iOS 26 SDK required for uploads (since Apr 28, 2026) | |
