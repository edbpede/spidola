---
type: "agent_requested"
description: "Rust coding guidelines"
---
# Idiomatic Rust on 1.96 / Edition 2024: A Capability Reference for Coding Agents

Rust 1.96 on the 2024 edition is the target (Rust 1.96.0 shipped May 28, 2026; the 1.96.1 point release followed with a Cargo and MIR-optimization fix). Rust's implementation posture is a monomorphizing, LLVM-backed, ahead-of-time compiler with an ownership-and-borrowing type system that eliminates data races and use-after-free at compile time with zero runtime cost. It is exceptional at zero-overhead abstractions, fearless concurrency, deterministic resource management (RAII via `Drop`), and encoding invariants into types so that illegal states do not compile. Optimize for making the compiler do the work: push correctness into the type system (newtypes, enums, typestate), let iterators and `impl Trait` compile down to hand-written-loop performance, and reserve `unsafe` for tiny, audited, encapsulated cores.

Agents most often write wrong-but-plausible Rust by importing habits from adjacent ecosystems: reaching for `.clone()` or `Rc<RefCell<T>>` to silence the borrow checker (a C++/GC reflex) instead of restructuring ownership; wrapping everything in `Arc<Mutex<T>>` because that is how you share in Go/Java; returning `Box<dyn Error>` and `.unwrap()`-ing everywhere as if exceptions existed; writing manual `for i in 0..v.len()` index loops instead of iterator chains; using stringly-typed data instead of enums; and blocking (`std::fs`, `std::thread::sleep`, heavy CPU) inside `async` tasks. This document shows the modern way once, concretely, so you can pattern-match to correct code.

## Edition 2024 baseline: what is now idiomatic

Every new crate targets `edition = "2024"` with `resolver = "3"` (implied by the edition). The edition changes that alter how you write code day to day:

- **`if let` chains are stable (Rust 1.88.0, released June 26, 2025 — edition 2024 only).** Chain `&&`-separated `let` bindings with boolean conditions in `if` and `while`. This depends on the 2024 `if let` temporary-scope change, which is why it is edition-gated.
- **RPIT lifetime capture (RFC 3498).** In Rust 2024, `-> impl Trait` return types implicitly capture *all* in-scope generic and lifetime parameters. Use precise-capturing `use<...>` bounds (stable since 1.82) to opt out of over-capturing.
- **`unsafe extern` blocks are required** and **`no_mangle` / `export_name` / `link_section` must be wrapped in `unsafe(...)`.**
- **`Future` and `IntoFuture` are in the prelude** — no more `use std::future::Future;` in most async code.
- **`gen` is a reserved keyword.** `gen` blocks are *not* stable as of 1.96 — do not use them in production; write manual `Iterator` impls or `std::iter::from_fn`.
- **Never-type fallback** changed: the `never_type_fallback_flowing_into_unsafe` lint is now deny-by-default, and `!` coerces more consistently.
- **`if let` temporaries drop at the end of the `if`/`else`**, not at the end of the enclosing statement — this fixes a class of `RwLock`/`MutexGuard` deadlocks in `else` branches.

```rust
// if let chains (Rust 1.88, edition 2024): flatten nested matches
fn describe(v: &serde_json::Value) -> Option<i64> {
    if let Some(obj) = v.as_object()
        && let Some(inner) = obj.get("count")
        && let Some(n) = inner.as_i64()
        && n > 0
    {
        Some(n)
    } else {
        None
    }
}
```

```rust
// RPIT precise capturing: return an iterator that does NOT capture `'a`
fn evens<'a>(slice: &'a [i32]) -> impl Iterator<Item = i32> + use<> {
    // `use<>` captures nothing; the returned iterator is 'static-friendly
    slice.iter().copied().filter(|n| n % 2 == 0).collect::<Vec<_>>().into_iter()
}

// Common case: capture only what the hidden type needs
fn names<'a>(users: &'a [String]) -> impl Iterator<Item = &'a str> + use<'a> {
    users.iter().map(String::as_str)
}
```

## Ownership, borrowing, and lifetimes

The default is to take the least-owning parameter that works: `&str` over `&String`, `&[T]` over `&Vec<T>`, `&T` over `T`, and return owned values. Elision handles the overwhelming majority of lifetimes; write explicit lifetimes only when the compiler cannot infer the relationship between input and output references, or when a struct holds a reference.

```rust
// Take slices, not owned collections, as parameters.
fn longest_word(text: &str) -> &str {
    text.split_whitespace().max_by_key(|w| w.len()).unwrap_or("")
}

// Explicit lifetime is required only when a struct borrows.
struct Parser<'src> {
    remaining: &'src str,
    line: usize,
}

impl<'src> Parser<'src> {
    fn new(input: &'src str) -> Self {
        Self { remaining: input, line: 1 }
    }
    // Elision: output borrows from &self, no annotation needed.
    fn peek(&self) -> Option<char> {
        self.remaining.chars().next()
    }
}
```

Critical insight: if you find yourself adding lifetimes to *function generics* to satisfy the checker, first ask whether the value should simply be owned or cloned once at a boundary. Lifetime soup in signatures is usually a design smell, not a requirement.

## Pattern matching and control flow

`match` is exhaustive and is the primary control-flow tool. Use `let else` (stable 1.65) for the "bind or diverge" pattern, or-patterns to collapse arms, binding modes (match ergonomics) so you rarely write `ref`/`&` by hand, and `@` bindings to capture while testing.

```rust
use std::net::IpAddr;

#[derive(Debug)]
enum Event {
    Connect { addr: IpAddr, port: u16 },
    Disconnect(u64),
    Heartbeat,
}

fn handle(ev: &Event) -> String {
    match ev {
        // Struct pattern with a guard.
        Event::Connect { addr, port } if *port == 443 => {
            format!("secure connect from {addr}")
        }
        Event::Connect { addr, port } => format!("connect {addr}:{port}"),
        // Or-pattern + @ binding.
        Event::Disconnect(id @ (0 | 1)) => format!("system disconnect {id}"),
        Event::Disconnect(id) => format!("disconnect {id}"),
        Event::Heartbeat => "ping".to_string(),
    }
}

// let-else: bind or return early. No rightward drift.
fn parse_port(s: &str) -> Option<u16> {
    let Ok(port) = s.parse::<u16>() else {
        return None;
    };
    (port != 0).then_some(port)
}
```

## Error handling

The idiomatic split is firm: **libraries define their own error enums with `thiserror` (v2.x)**; **applications use `anyhow` (1.x)** (or `color-eyre` when you want pretty, span-aware reports). `thiserror` generates the `std::error::Error` impl and never leaks into your public API. `anyhow::Result<T>` is a drop-in for the ad-hoc case, with `.context()` for a semantic trace. The `failure` crate is dead — never use it. `error-chain` is likewise obsolete.

```rust
// LIBRARY: a precise, matchable error enum with thiserror 2.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("key `{0}` not found")]
    NotFound(String),
    #[error("record is corrupt at offset {offset}")]
    Corrupt { offset: u64 },
    // `#[from]` generates From<std::io::Error> and wires up `source()`.
    #[error("I/O failure")]
    Io(#[from] std::io::Error),
}

pub fn load(key: &str) -> Result<Vec<u8>, StoreError> {
    let bytes = std::fs::read(key)?; // io::Error -> StoreError via #[from]
    if bytes.is_empty() {
        return Err(StoreError::NotFound(key.to_string()));
    }
    Ok(bytes)
}
```

```rust
// APPLICATION: anyhow with context. `?` composes across many error types.
use anyhow::{Context, Result, bail};

fn run(config_path: &str) -> Result<()> {
    let text = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading config from {config_path}"))?;
    let port: u16 = text.trim().parse().context("config must be a port number")?;
    if port < 1024 {
        bail!("refusing privileged port {port}");
    }
    Ok(())
}
```

Rules: never `unwrap()`/`expect()` in library code paths that can fail at runtime — return `Result`. Reserve `expect()` for genuine invariants ("checked above", "compile-time constant") with a message stating *why* it cannot fail. Prefer concrete error enums over `Box<dyn Error>`; use `Box<dyn Error + Send + Sync>` only in the tiniest throwaway binaries where `anyhow` would also serve.

## Traits: dispatch, impl Trait, and the modern trait toolbox

Choose associated types when a trait has exactly one output type per implementer (like `Iterator::Item`); use generic parameters when a type implements the trait many ways. Prefer static dispatch (`impl Trait` / generics) by default; reach for `dyn` trait objects only for heterogeneous collections or to break compile-time fan-out.

- **RPITIT** (return-position `impl Trait` in traits) and **async fn in traits** are native and stable (1.75). Use them directly; drop `#[async_trait]` unless you need a `dyn`-dispatched async trait.
- **GATs** (generic associated types) stable 1.65 — for lending iterators and similar.
- **Trait upcasting** stable in Rust 1.86.0 (released April 3, 2025) — coerce `&dyn Sub` to `&dyn Super` (and `Arc`/`Box`/`Rc`), invaluable for upcasting custom traits to `dyn Any`.

```rust
// impl Trait in argument (APIT) and return position.
fn total(items: impl IntoIterator<Item = u32>) -> u32 {
    items.into_iter().sum()
}

// Native async fn in traits (stable 1.75) — no async-trait crate.
trait Fetcher {
    async fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>>;
    // RPITIT: return an opaque iterator from a trait method.
    fn cached_keys(&self) -> impl Iterator<Item = &str>;
}

// Sealed trait: implementable only within this crate.
mod sealed {
    pub trait Sealed {}
}
pub trait Codec: sealed::Sealed {
    fn id(&self) -> u32;
}
// Downstream crates can name Codec but cannot implement it.

// Blanket impl: give every Display type a `.to_label()`.
trait Label {
    fn to_label(&self) -> String;
}
impl<T: std::fmt::Display> Label for T {
    fn to_label(&self) -> String {
        format!("[{self}]")
    }
}
```

When to use `dyn`: heterogeneous storage (`Vec<Box<dyn Draw>>`), plugin boundaries, and reducing monomorphization bloat. Note that a trait with RPITIT or async fn is *not* `dyn`-compatible unless you box the futures — for `dyn` async, keep `#[async_trait]`.

## Closures, iterators, and adapters

Iterators are lazy and compile to the same code as hand-written loops. Never write manual index loops over collections. Build pipelines with adapters; terminate with `collect`, `sum`, `fold`, `for_each`, or `try_fold`. Use `collect::<Result<Vec<_>, _>>()` to short-circuit on the first error.

```rust
use std::collections::HashMap;

// Group words by length, counting — no manual loops, no manual indexing.
fn length_histogram(text: &str) -> HashMap<usize, usize> {
    text.split_whitespace()
        .map(str::len)
        .fold(HashMap::new(), |mut acc, len| {
            *acc.entry(len).or_insert(0) += 1;
            acc
        })
}

// Collect into Result: stops at the first parse failure.
fn parse_all(lines: &[&str]) -> Result<Vec<i64>, std::num::ParseIntError> {
    lines.iter().map(|s| s.parse::<i64>()).collect()
}
```

Use `itertools` when std lacks the adapter you need (`chunk_by`, `itertools::izip!`, `sorted`, `unique`, `join`). Avoid `.collect()` into a temporary `Vec` just to iterate again — chain directly.

## Const generics, const fn, and inline const

Const generics parameterize over values; `const fn` runs at compile time; inline `const { ... }` blocks force compile-time evaluation in expression position.

```rust
// Const-generic fixed-size buffer.
struct RingBuffer<T, const N: usize> {
    data: [Option<T>; N],
    head: usize,
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    const fn new() -> Self {
        Self { data: [None; N], head: 0 }
    }
}

// Compile-time table via inline const (stable 1.79).
fn lookup(i: usize) -> u32 {
    const TABLE: [u32; 4] = { let mut t = [0; 4]; t[1] = 10; t[2] = 20; t[3] = 30; t };
    TABLE[i]
}
```

## Smart pointers: choosing the container

| Need | Use | Notes |
|------|-----|-------|
| Single owner, heap allocation / recursion | `Box<T>` | Cheapest indirection; enables recursive types and `dyn`. |
| Shared ownership, single thread | `Rc<T>` | Non-atomic refcount; not `Send`. |
| Shared ownership across threads | `Arc<T>` | Atomic refcount. Wrap in `Mutex`/`RwLock` only if mutable. |
| Interior mutability, single thread, `Copy` values | `Cell<T>` | No borrows handed out; get/set. |
| Interior mutability, single thread, borrows | `RefCell<T>` | Runtime borrow checking; panics on violation. |
| Borrow-or-own at a boundary | `Cow<'a, T>` | Avoids allocation when a borrow suffices. |

Critical insight: `Rc<RefCell<T>>` and `Arc<Mutex<T>>` are the "I gave up on ownership" pattern. They are correct for genuine shared-mutable graphs (some UI trees, caches), but if you reach for them to fix a borrow-check error in otherwise tree-shaped data, restructure instead — pass `&mut`, split borrows, or use indices into a `Vec` (arena pattern).

```rust
use std::borrow::Cow;

// Cow: only allocate when we actually change something.
fn normalize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))
    } else {
        Cow::Borrowed(input)
    }
}
```

## Idiomatic patterns: newtype, typestate, builder

```rust
// Newtype: make illegal states unrepresentable and add semantic meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(u64);

#[derive(Debug, Clone, Copy)]
pub struct Celsius(f64);
// A function taking Celsius cannot be passed a raw f64 by accident.

// Typestate: encode protocol state in the type so misuse won't compile.
struct Locked;
struct Unlocked;
struct Door<State> { _marker: std::marker::PhantomData<State> }

impl Door<Locked> {
    fn unlock(self) -> Door<Unlocked> { Door { _marker: std::marker::PhantomData } }
}
impl Door<Unlocked> {
    fn open(&self) { println!("opening"); }
    fn lock(self) -> Door<Locked> { Door { _marker: std::marker::PhantomData } }
}

// Builder: for structs with many optional fields.
#[derive(Default)]
pub struct ServerBuilder {
    host: Option<String>,
    port: Option<u16>,
    tls: bool,
}
impl ServerBuilder {
    pub fn host(mut self, h: impl Into<String>) -> Self { self.host = Some(h.into()); self }
    pub fn port(mut self, p: u16) -> Self { self.port = Some(p); self }
    pub fn tls(mut self, on: bool) -> Self { self.tls = on; self }
    pub fn build(self) -> Server {
        Server { host: self.host.unwrap_or_else(|| "0.0.0.0".into()),
                 port: self.port.unwrap_or(8080), tls: self.tls }
    }
}
pub struct Server { host: String, port: u16, tls: bool }
```

Derive `#[derive(bon::Builder)]` (the `bon` crate) for generated, compile-checked builders when hand-writing gets tedious.

## Async Rust with Tokio

Tokio is the dominant runtime; its LTS policy designates `1.47.x` as LTS until September 2026 (MSRV 1.70) and `1.51.x` until March 2027 (MSRV 1.71). For a new project just depend on `tokio = { version = "1", ... }`. Enter it with `#[tokio::main]`; spawn concurrent tasks with `tokio::spawn`; coordinate with `select!` and the channel family. Structured concurrency comes from `JoinSet` (dynamic task sets) and cancellation tokens.

```rust
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // mpsc: many producers, one consumer.
    let (tx, mut rx) = mpsc::channel::<u32>(32);

    // watch: broadcast latest value; ideal for config/shutdown state.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // JoinSet: structured, awaitable set of tasks.
    let mut set = JoinSet::new();
    for id in 0..4 {
        let tx = tx.clone();
        let mut shutdown = shutdown_rx.clone();
        set.spawn(async move {
            loop {
                tokio::select! {
                    _ = sleep(Duration::from_millis(50)) => {
                        if tx.send(id).await.is_err() { break; }
                    }
                    _ = shutdown.changed() => break,
                }
            }
        });
    }
    drop(tx); // so rx closes when workers exit

    // Consume a few, then signal graceful shutdown.
    let mut seen = 0;
    while let Some(v) = rx.recv().await {
        seen += 1;
        if seen >= 8 {
            let _ = shutdown_tx.send(true);
            break;
        }
        let _ = v;
    }
    while set.join_next().await.is_some() {} // await all workers
    Ok(())
}
```

Channel selection:

| Channel | Shape | Use for |
|---------|-------|---------|
| `mpsc` | many→one, bounded/unbounded | work queues, task pipelines |
| `oneshot` | one value, one→one | request/response, returning a result from a spawned task |
| `broadcast` | many→many, each receiver sees all | fan-out events (with lag handling) |
| `watch` | many receivers see latest only | config reload, shutdown flags |

**Async pitfalls to avoid:**
- **Never block the runtime.** No `std::thread::sleep`, blocking file I/O, or heavy CPU on a worker thread — use `tokio::time::sleep`, `tokio::fs`, and offload CPU work to `tokio::task::spawn_blocking` or `rayon`.
- **Never hold a `std::sync::Mutex`/`RwLock` guard across `.await`** — the guard is not `Send` and you can deadlock. Either use `tokio::sync::Mutex` (async-aware) or scope the lock so it drops before the await point.
- Add `Send + 'static` bounds to futures you `spawn`; most cross-thread executors require them.
- Use `Pin`/`Unpin` explicitly only when implementing `Future`/`Stream` by hand or storing self-referential futures; day-to-day async never touches them.

```rust
// Correct: scope the std lock so it is released before awaiting.
use std::sync::Mutex;
async fn update(counter: &Mutex<u64>) {
    {
        let mut g = counter.lock().unwrap();
        *g += 1;
    } // guard dropped here
    tokio::task::yield_now().await; // safe: no guard held across await
}
```

Streams: use `tokio_stream` and the `futures::Stream` trait; `tokio_stream::StreamExt` gives `.next()`, `.map`, `.filter`. Convert channels/intervals to streams with `tokio_stream::wrappers`.

`async fn in traits` (native, 1.75) covers most trait needs. Use `async-trait` only when the trait object must be `dyn`-dispatched (boxed futures).

## Concurrency without async

For CPU-bound and thread-based work:

- **Scoped threads** (`std::thread::scope`, stable 1.63) borrow local data without `'static` or `Arc`.
- **`rayon`** for data parallelism: `.par_iter()` turns a sequential iterator pipeline parallel.
- **`Arc<Mutex<T>>` / `Arc<RwLock<T>>`** for shared mutable state; `RwLock` when reads dominate.
- **Channels**: `std::sync::mpsc` for simple cases; `crossbeam::channel` for multi-consumer, `select!`, and better performance.
- **Atomics** (`AtomicU64`, `AtomicBool`, …) for lock-free counters/flags with an explicit `Ordering`.

```rust
use std::thread;

fn parallel_sum(data: &[i64]) -> i64 {
    let mid = data.len() / 2;
    let (a, b) = data.split_at(mid);
    thread::scope(|s| {
        let h = s.spawn(|| a.iter().sum::<i64>()); // borrows `a`, no Arc
        let right: i64 = b.iter().sum();
        h.join().unwrap() + right
    })
}
```

```rust
// rayon: trivial data parallelism.
use rayon::prelude::*;
fn sum_squares(v: &[f64]) -> f64 {
    v.par_iter().map(|x| x * x).sum()
}
```

Note: the standard library's `Mutex`/`RwLock` were reimplemented to be small and fast, which has significantly narrowed the historical gap with `parking_lot`. Prefer `std` locks by default; reach for `parking_lot` only for its specific extras (no poisoning, `const` constructors on older toolchains, fairness options). For a concurrent map, use `dashmap` rather than wrapping a `HashMap` in a global lock.

## Serialization with serde

`serde` + `serde_derive` (1.x) is the universal standard; pair with `serde_json` (1.x) for JSON. Derive `Serialize`/`Deserialize`, drive shape with attributes, and exploit zero-copy borrows for parse-heavy workloads.

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    listen_port: u16,
    #[serde(default)]
    verbose: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_file: Option<String>,
    #[serde(rename = "maxConnections", default = "default_max")]
    max_connections: u32,
}
fn default_max() -> u32 { 1024 }

fn parse(json: &str) -> serde_json::Result<Config> {
    serde_json::from_str(json)
}
```

```rust
// Zero-copy: borrow &str straight out of the input buffer, no allocation.
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct LogLine<'a> {
    level: &'a str,
    #[serde(borrow)]
    message: std::borrow::Cow<'a, str>,
}
// Deserialize from `&'de str`; `level` points into the original buffer.
```

For fully custom formats, implement `Serialize`/`Deserialize` by hand or use `#[serde(with = "...")]`/`serialize_with`/`deserialize_with` per field. Use `serde_bytes` for efficient byte arrays.

## The ecosystem: current winners and what to avoid

| Job | Use | Avoid / superseded |
|-----|-----|--------------------|
| Async runtime | `tokio` (1.x) | — |
| Web framework | `axum` (0.8.x) | `actix-web` is fine and fast but `axum` is the default choice for new Tokio-stack services; `warp` is largely dormant |
| HTTP client | `reqwest` | raw `hyper` unless you need low-level control |
| CLI parsing | `clap` v4 (derive API) | `structopt` (merged into clap; do not use), `argh` only for tiny/no-dep needs |
| Serialization | `serde` (+ `serde_json`) | — |
| Library errors | `thiserror` v2 | `failure` (dead), `error-chain` (obsolete) |
| App errors | `anyhow` / `color-eyre` | `Box<dyn Error>` for non-trivial apps |
| Logging/observability | `tracing` + `tracing-subscriber` | `log` + `env_logger` (fine for small CLIs/libs) |
| Date/time | `jiff` (new, timezone-complete) or `chrono` | raw `time` unless already in your graph |
| Random | `rand` (0.9.x) | — |
| Concurrent map | `dashmap` | `Mutex<HashMap>` on hot paths |
| Ordered/insertion map | `indexmap` | — |
| Small-vector optimization | `smallvec` | — |
| Faster locks (situational) | `parking_lot` | now often unnecessary vs std |
| SQL | `sqlx` / `sea-orm` / `diesel` | see decision below |
| Regex | `regex` | hand-rolled parsing of regular languages |
| UUID | `uuid` | — |

**Web (`axum` 0.8):** per the Tokio blog announcing axum 0.8.0 (January 1, 2025), "the path parameter syntax has changed from `/:single` and `/*many` to `/{single}` and `/{*many}`" (introduced with the upgrade to `matchit` 0.8). Handlers are plain async functions; extractors (`Path`, `Query`, `Json`, `State`) are type-checked. axum 0.8 uses native async traits — no `#[async_trait]` on extractors.

```rust
use axum::{routing::get, Router, extract::{Path, State}, Json};
use std::sync::Arc;

#[derive(Clone)]
struct AppState { greeting: Arc<String> }

async fn hello(Path(name): Path<String>, State(st): State<AppState>) -> Json<String> {
    Json(format!("{}, {name}", st.greeting))
}

pub fn app() -> Router {
    let state = AppState { greeting: Arc::new("Hello".into()) };
    Router::new().route("/hello/{name}", get(hello)).with_state(state)
}
```

**CLI (`clap` v4 derive):**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve { #[arg(long)] detach: bool },
    Migrate,
}

fn main() {
    let cli = Cli::parse();
    // dispatch on cli.command …
}
```

**Logging/tracing:** use `tracing` for anything async or structured; `tracing-subscriber` with `EnvFilter` reads `RUST_LOG`. The older `log` + `env_logger` still suffices for a synchronous CLI or a small library (libraries should emit via `log` *or* `tracing` and let the binary choose the subscriber).

```rust
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

fn init_telemetry() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
}
```

**Date/time:** `chrono` remains the widest-integrated choice (DB drivers, HTTP libs). For new code that needs correct, complete timezone handling, `jiff` is the modern recommendation. Reach for the bare `time` crate mainly when a dependency already forces it.

**Random (`rand` 0.9):** the 0.9 API renamed the ergonomic entry points — use `rand::rng()` (not `thread_rng`), `.random()` / `.random_range()` (not `gen`/`gen_range`), and `from_os_rng` for seeding. Slice shuffling moved to the `IndexedRandom`/`IndexedMutRandom` traits.

```rust
use rand::Rng;
fn roll() -> u32 {
    let mut rng = rand::rng();
    rng.random_range(1..=6)
}
```

**Databases — decision:**

| Library | Model | Choose when |
|---------|-------|-------------|
| `sqlx` (0.8.x) | async, raw SQL, compile-time-checked queries | you want async + direct SQL control and don't need an ORM |
| `sea-orm` (2.x) | async, ActiveRecord-style ORM on top of sqlx | you want relationship modeling and Rails/Django-like ergonomics |
| `diesel` (2.x) | sync (async via `diesel-async`), typed query DSL | you want the strongest compile-time query guarantees; sync-first |

Note: async DB drivers do not automatically outperform sync ones; the meaningful perf lever is query pipelining, which `diesel-async`/`tokio-postgres` support but `sqlx`/`sea-orm` do not.

## Project structure, modules, and visibility

Use the modern module layout: a module `foo` lives in `foo.rs`, and its children live in a sibling `foo/` directory. **Do not create `mod.rs` files** — that is the legacy 2015 style. Split binary and library: put reusable logic in `src/lib.rs` and keep `src/main.rs` a thin shell that calls into the library (this makes the logic testable and reusable).

```text
myapp/
├── Cargo.toml
├── src/
│   ├── lib.rs          # pub API surface, `pub mod` declarations
│   ├── main.rs         # thin binary: parse args, call into lib
│   ├── config.rs       # module `config`
│   ├── config/         # children of `config`
│   │   └── loader.rs   # module `config::loader`
│   └── store.rs
└── tests/
    └── integration.rs  # black-box tests against the public API
```

Visibility: default to private; expose deliberately. Use `pub(crate)` for cross-module-but-internal items, `pub(super)` for parent-only, and re-export a clean public surface from `lib.rs` with `pub use`.

```rust
// src/lib.rs
pub mod config;
mod store; // private module …

pub use store::Store; // … but re-export the one public type.
```

Workspaces: for multi-crate projects, use a virtual manifest with shared dependency versions.

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
```

```toml
# crates/api/Cargo.toml (member)
[package]
name = "api"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
tokio = { workspace = true }
serde = { workspace = true }

[lints]
workspace = true   # NOT inherited automatically — must opt in explicitly
```

## Tooling configuration (copy-ready)

**Cargo.toml — single-crate release tuning:**

```toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2024"
rust-version = "1.96"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"

[profile.release]
opt-level = 3
lto = "thin"        # "fat" (or true) for max speed; "off" disables entirely; false ≠ off
codegen-units = 1   # better optimization at the cost of build parallelism
panic = "abort"     # smaller/faster if you don't need unwinding ("unwind" is default)
strip = "symbols"   # "none" | "debuginfo" | "symbols"

# Custom profile that inherits and overrides.
[profile.release-fast]
inherits = "release"
lto = "fat"

[lints.rust]
unsafe_code = "warn"

[lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
```

`lto` values: `false` (default; "thin local" LTO only), `true`/`"fat"` (whole-graph), `"thin"`, and `"off"` (fully disabled — distinct from `false`). `strip` accepts `"none"`/`"debuginfo"`/`"symbols"` (or `true` ≡ `"symbols"`). Per-package profile overrides cannot set `panic`, `lto`, or `rpath`.

Prefer the `[lints]` table (stable 1.74) over scattering `#![warn(...)]` attributes — it is per-package, tool-aware (`lints.rust`, `lints.clippy`, `lints.rustdoc`), and inheritable via `[workspace.lints]`. The `priority = -1` trick lets a group (like `clippy::pedantic`) be set at a base level while individual lints override it.

**rustfmt.toml:**

```toml
edition = "2024"
max_width = 100
```

(Options like `imports_granularity` and `group_imports` are useful but currently unstable in rustfmt — they require nightly rustfmt. On stable, keep `rustfmt.toml` minimal and let defaults handle the rest.)

**clippy.toml** (behavioral thresholds that the `[lints]` table cannot express):

```toml
msrv = "1.96"
too-many-arguments-threshold = 8
cognitive-complexity-threshold = 30
```

**rust-toolchain.toml** (pin the toolchain for reproducible builds):

```toml
[toolchain]
channel = "1.96.1"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

**Everyday commands and extensions:**

| Command | Purpose |
|---------|---------|
| `cargo clippy --all-targets -- -D warnings` | lint, failing on any warning |
| `cargo fmt --all` | format |
| `cargo nextest run` | faster, per-process test runner |
| `cargo machete` | find unused dependencies |
| `cargo deny check` | license / advisory / ban policy |
| `cargo audit` | RUSTSEC vulnerability scan |
| `cargo expand` | see macro-expanded source |

## Testing

The built-in harness (`#[test]`, `#[cfg(test)]` modules, `tests/` for integration) is the baseline. Run it with `cargo nextest run` (faster, per-test process isolation, retries) — nextest is a drop-in for `cargo test` (except doctests, which stay on `cargo test`). Layer in:

- **`rstest`** — parametrized cases and fixtures.
- **`insta`** — snapshot testing (review with `cargo insta review`).
- **`proptest`** / `quickcheck` — property-based testing.
- **`criterion`** — statistically rigorous benchmarks (in `benches/`).
- **`mockall`** — mock objects for trait dependencies.
- **`assert_cmd`** — black-box testing of CLI binaries.

```rust
// rstest: one test body, many cases + a fixture.
use rstest::{fixture, rstest};

#[fixture]
fn base() -> Vec<i32> { vec![1, 2, 3] }

#[rstest]
#[case(0, 1)]
#[case(2, 3)]
fn indexes(base: Vec<i32>, #[case] idx: usize, #[case] expected: i32) {
    assert_eq!(base[idx], expected);
}

// Standard unit test with the built-in harness.
#[cfg(test)]
mod tests {
    #[test]
    fn parses_port() {
        assert_eq!(super::parse_port("443"), Some(443));
    }
}
```

## Unsafe Rust, done right

Use `unsafe` only when there is no safe alternative: FFI, hand-tuned data structures, or SIMD/intrinsics. Encapsulate it behind a safe API, keep the unsafe region minimal, and annotate every unsafe operation with a `// SAFETY:` comment stating which invariant makes it sound.

In Edition 2024, **`extern` blocks must be `unsafe extern`**, individual items are `safe fn`/`unsafe fn` (unqualified defaults to unsafe), and **`no_mangle`/`export_name`/`link_section` must be wrapped in `unsafe(...)`**.

```rust
// Edition 2024 unsafe extern block.
unsafe extern "C" {
    // Callable without an unsafe block: any f64 is valid.
    pub safe fn sqrt(x: f64) -> f64;
    // Requires a valid pointer, so callers must use `unsafe`.
    pub unsafe fn strlen(p: *const std::ffi::c_char) -> usize;
    // No qualifier → implicitly unsafe.
    pub fn free(p: *mut core::ffi::c_void);
}

// Edition 2024: unsafe-wrapped attributes.
// SAFETY: no other global symbol uses this name.
#[unsafe(no_mangle)]
pub extern "C" fn my_plugin_entry() {}

#[unsafe(export_name = "compute_v2")]
pub extern "C" fn compute() {}
```

```rust
use std::mem::MaybeUninit;

// MaybeUninit: build an array without initializing it twice.
fn init_array() -> [u32; 4] {
    let mut arr: [MaybeUninit<u32>; 4] = [const { MaybeUninit::uninit() }; 4];
    for (i, slot) in arr.iter_mut().enumerate() {
        slot.write(i as u32 * 10);
    }
    // SAFETY: every element was written exactly once in the loop above.
    unsafe { std::mem::transmute::<_, [u32; 4]>(arr) }
}
```

Respect pointer provenance: derive pointers from the allocation you intend to access (use `wrapping_add`/`add` on the right base, don't fabricate addresses via integer casts). When NOT to use unsafe: to skip bounds checks the optimizer already elides, to "speed up" safe code you haven't profiled, or to work around the borrow checker. Test every unsafe crate under **Miri** (`cargo +nightly miri test`) to catch UB, and consider a `#![forbid(unsafe_code)]` at crate level for crates that should have none.

## Performance idioms

- **Prefer borrowing to owning.** `&str`/`&[T]` parameters avoid allocation; return `Cow` when the result may or may not need to own.
- **Pre-allocate** with `Vec::with_capacity` / `String::with_capacity` when the size is known — resizing reallocates and copies.
- **Iterators are zero-cost** and usually beat manual loops; don't fear chains of adapters.
- **`.clone()` is fine** at boundaries, for small `Copy`-ish types, or once to break a borrow knot cheaply. It is a *smell* when it appears in a hot loop or repeatedly to appease the checker — that signals an ownership design problem.
- **`#[inline]`** on small cross-crate hot functions can help; measure with `criterion` before sprinkling it. Trust the optimizer for within-crate calls.
- Choose data structures deliberately: `smallvec` for collections usually ≤ N inline, `indexmap` when insertion order matters, `Box<[T]>` over `Vec<T>` for immutable owned slices to save the capacity word.

```rust
// Pre-allocate when the final size is known.
fn render(rows: &[&str]) -> String {
    let cap: usize = rows.iter().map(|r| r.len() + 1).sum();
    let mut out = String::with_capacity(cap);
    for r in rows {
        out.push_str(r);
        out.push('\n');
    }
    out
}
```

## Anti-patterns to avoid

- **`.clone()` to fight the borrow checker.** Restructure ownership, split borrows, or pass `&mut`. Clone deliberately, not reflexively.
- **`.unwrap()`/`.expect()` in library code.** Return `Result`. Reserve `expect` for documented invariants, with a message explaining why failure is impossible.
- **`Arc<Mutex<T>>` by default.** Most data has a single owner; pass references. Use shared-mutable only for genuinely shared state, and prefer message passing (channels) or `dashmap` where they fit.
- **`Rc<RefCell<T>>` for tree data.** If your data is actually tree-shaped, use ownership + `&mut`, or an arena (`Vec<Node>` with index "pointers"). Reserve `Rc<RefCell>` for true graphs.
- **Importing foreign concurrency/error habits.** No goroutine-style "spawn and forget" without a `JoinSet`/handle; no exception-style `unwrap` cascades; no Python-style dynamic typing via `HashMap<String, Value>` where a struct/enum fits.
- **Stringly-typed code.** Replace magic strings and `bool` flags with enums and newtypes so the compiler enforces the domain.
- **Premature `unsafe`.** Almost never needed in application code. Profile first; the safe version is usually as fast.
- **Blocking in async.** No `std::thread::sleep`, blocking I/O, or CPU-bound loops on runtime threads; use async equivalents or `spawn_blocking`.
- **Holding a lock across `.await`.** Scope the guard to drop before the await, or use `tokio::sync` locks.
- **Manual index loops** (`for i in 0..v.len()`). Use iterators; index only when the algorithm genuinely needs positions (and even then prefer `.enumerate()`).
- **`Deref` abuse.** Implement `Deref`/`DerefMut` only for genuine smart-pointer/newtype-wrapper types, never to fake inheritance or expose an inner type's whole API by accident.
- **Over-genericization / trait soup.** Don't add generic parameters or trait bounds "for flexibility" you don't need. Start concrete; generalize when a second concrete use appears.

## Version & compatibility

- Research date: July 7, 2026
- Research basis: current official docs, release notes, specifications, changelogs, and primary repositories.

| Feature / tool | Status on this stack | Version floor |
|----------------|----------------------|---------------|
| Rust toolchain | target | 1.96 / 1.96.1 stable |
| Edition | target | 2024 (stable since 1.85, Feb 20 2025) |
| Cargo resolver | default for edition 2024 | resolver 3 |
| `let`-else | stable | 1.65 |
| GATs | stable | 1.65 |
| RPITIT + async fn in traits | stable | 1.75 |
| `[lints]` table in Cargo.toml | stable | 1.74 |
| Inline `const { }` expressions | stable | 1.79 |
| Precise capturing `use<...>` | stable | 1.82 |
| `unsafe extern` / `unsafe(attr)` | required in edition 2024 | syntax since 1.82 |
| Async closures (`async \|\|`) | stable | 1.85 (Feb 20 2025) |
| Trait upcasting (`dyn` to supertrait) | stable | 1.86 (Apr 3 2025) |
| `if let` chains | stable, edition 2024 only | 1.88 (Jun 26 2025) |
| `gen` blocks | NOT stable — do not use | reserved keyword in 2024 |
| tokio | 1.x (LTS 1.47 to Sep 2026, 1.51 to Mar 2027) | — |
| axum | 0.8.x | Rust 1.80 (per crates.io) |
| clap | 4.x (derive) | — |
| thiserror | 2.x | — |
| anyhow | 1.x | — |
| serde / serde_json | 1.x | — |
| rand | 0.9.x | — |
| sqlx / sea-orm / diesel | 0.8.x / 2.x / 2.x | — |
