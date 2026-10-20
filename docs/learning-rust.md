# Learning Rust with Over There

This guide explains the Rust you will meet in this project, using real code from this repository. Read it next to the source. [architecture.md](architecture.md) explains *why* the app is shaped this way; this file explains *how the Rust works*.

If you are brand new to Rust, keep [The Rust Book](https://doc.rust-lang.org/book/) open alongside. Chapter numbers below point to it.

## 1. Where things are

The folder tree, the layers and the dependency rule are in [architecture.md](architecture.md#folder-structure). In short: `src/domain/` holds pure rules, `src/identity/` holds accounts, `src/infrastructure/` holds SQL, and `src/presentation/web.rs` is the HTTP layer that ties them together.

### Suggested reading order

1. [src/main.rs](../src/main.rs): 9 lines, the whole program start.
2. [src/domain/route.rs](../src/domain/route.rs): enums, structs, `match`, `Result`, tests. The gentlest real Rust file here.
3. [src/domain/gpx.rs](../src/domain/gpx.rs): borrowing, iterators, `Option`.
4. [src/infrastructure/routes.rs](../src/infrastructure/routes.rs): `async`, SQLx, derive macros.
5. [src/presentation/web.rs](../src/presentation/web.rs): start at `run()` and `router()`, then read one handler (`route_detail`) end to end.

## 2. How a request flows through the code

Take `GET /routes/coastal-loop-3f2a…`:

1. **Startup.** `main` calls `presentation::web::run()` ([web.rs:278](../src/presentation/web.rs#L278)). It reads `.env`, opens the SQLite pool, runs `migrations/`, loads `templates/page.hbs`, builds an `AppState`, and starts listening.
2. **Routing.** `router()` ([web.rs:346](../src/presentation/web.rs#L346)) maps `"/routes/{slug}"` to the `route_detail` function.
3. **Extraction.** Axum calls `route_detail(State(state), headers, Path(slug))`. It pulls those arguments out of the request based on their *types* (see §4.10).
4. **Who is asking?** `viewer()` looks up the session cookie via `identity::auth::session`.
5. **Load data.** `get_route()` calls `infrastructure::routes::find_by_slug`, which runs SQL and returns a `RouteRecord`.
6. **Domain rule.** `can_view()` turns the record into domain values and asks `domain::route::may_view`. A private route the viewer cannot see returns `404`, so its existence is not leaked.
7. **Render.** Values go into a `serde_json::Value`, Handlebars fills `page.hbs`, and the HTML is returned.
8. **Errors.** Any `?` that hits an error returns an `AppError`, which Axum turns into an error page (§4.6).

Uploading (`POST /upload`, [web.rs:916](../src/presentation/web.rs#L916)) follows the same steps with more of them: check sign-in and CSRF, validate the form with `RouteDraft::from_fields`, parse with `gpx::parse`, hide endpoints with `gpx::public_segments`, write files, then insert rows inside a database transaction. If the insert fails, it deletes the files it just wrote.

## 3. Cargo: the build tool

- `Cargo.toml` lists dependencies. `features = [...]` turns on optional parts of a crate; for example, `sqlx` is built with only `sqlite`, `migrate`, `macros` and `chrono`.
- `[dev-dependencies]` (`tempfile`, `tower`) are compiled only for tests.
- `Cargo.lock` pins exact versions. `cargo build --locked` refuses to change it.
- A **crate** is one compiled package. This project is one binary crate called `over_there` (the `-` becomes `_`, which is why the log filter is `RUST_LOG=over_there=info`).

Everyday commands: `cargo check` (fast type check), `cargo test`, `cargo fmt`, `cargo clippy` (lints, worth running while learning), `cargo run`.

## 4. Rust concepts, using this code

### 4.1 Modules and visibility — Book ch. 7

[main.rs](../src/main.rs) declares the top-level modules:

```rust
mod domain;        // looks for src/domain/mod.rs (or src/domain.rs)
mod presentation;
```

[domain/mod.rs](../src/domain/mod.rs) declares the files inside it:

```rust
pub mod gpx;      // src/domain/gpx.rs
pub mod route;
```

Everything is **private by default**. `pub` makes it usable from outside its module. `AppState` and `AppError` in `web.rs` have no `pub`, so only `web.rs` can touch them, while `pub async fn run()` is called from `main`.

`use` brings names into scope. `crate::` means "from the root of this crate":

```rust
use crate::domain::route::{self, RouteDraft, Visibility};
// `self` lets you write route::slugify(...) as well as RouteDraft directly
```

### 4.2 Ownership and borrowing — Book ch. 4

Every value has one owner; when the owner goes out of scope, the value is freed. You can **borrow** a value with `&` (read-only) or `&mut` (exclusive, may change it) without taking ownership.

The most common pair you will see here is `String` (owned, growable text) and `&str` (a borrowed view of text):

```rust
// domain/route.rs — only reads the strings, so it borrows them
pub fn may_edit(owner_id: &str, actor_id: &str, is_admin: bool) -> bool {
    owner_id == actor_id || is_admin
}
```

The same goes for bytes: `gpx::parse(bytes: &[u8])` borrows the uploaded file instead of copying it. `&[T]` is a *slice*, a borrowed view into a `Vec<T>` or array.

When you need to keep data beyond the borrow, make an owned copy with `.to_owned()`, `.to_string()` or `.clone()`. `RouteDraft::from_fields` does this, because the draft must outlive the form map it reads from:

```rust
let get = |key: &str| fields.get(key).map(String::as_str).unwrap_or("").trim().to_owned();
```

In `download` ([web.rs:533](../src/presentation/web.rs#L533)), `let key = if owner { &r.gpx_key } else { ... }` borrows a field instead of cloning it, because it is only needed for the next line.

### 4.3 Structs, enums and `impl` — Book ch. 5, 6

A `struct` groups named fields; an `enum` is a value that is exactly one of several variants:

```rust
// domain/route.rs
pub enum Visibility { Public, Unlisted, Private }

impl Visibility {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "public" => Ok(Self::Public),
            "unlisted" => Ok(Self::Unlisted),
            "private" => Ok(Self::Private),
            _ => Err("Choose a visibility setting".into()),
        }
    }
}
```

- `impl` attaches functions to a type. `Visibility::parse(...)` has no `self`, so it is an *associated function* (like a static method). `draft.visibility_str()` takes `&self`, so it is a *method*.
- `match` must cover every case; `_` is the catch-all. If you add a variant to an enum, the compiler shows you every `match` you forgot to update.

`AppError(StatusCode, String)` in `web.rs` is a *tuple struct*: fields are accessed as `self.0` and `self.1`.

### 4.4 `#[derive]` — Book appendix C

`derive` asks the compiler to generate standard code for a type:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility { ... }
```

- `Debug` allows `{:?}` printing; `PartialEq` allows `==`; `Clone` allows `.clone()`; `Copy` means it is copied implicitly, like a number.
- Library derives work the same way: `#[derive(FromRow)]` on `RouteRecord` lets SQLx build it from a database row by column name, and `#[derive(Serialize)]` on `gpx::Point` lets serde turn it into JSON. `#[serde(skip)]` leaves the `time` field out.

### 4.5 `Option`, `Result` and `?` — Book ch. 6.1, 9

Rust has no `null` and no exceptions:

- `Option<T>` is `Some(value)` or `None`. For example, `ele: Option<f64>` because not every GPX point has elevation.
- `Result<T, E>` is `Ok(value)` or `Err(error)`.

The `?` operator means: if this is `Err` (or `None`), return it from the current function now; otherwise unwrap the value and continue. Here is `get_route` ([web.rs:257](../src/presentation/web.rs#L257)), which is worth reading slowly:

```rust
async fn get_route(pool: &SqlitePool, slug: &str) -> Result<RouteRecord, AppError> {
    routes::find_by_slug(pool, slug)   // Future<Output = Result<Option<RouteRecord>, sqlx::Error>>
        .await                         // Result<Option<RouteRecord>, sqlx::Error>
        .map_err(AppError::internal)?  // database error -> AppError, returned early; now Option<RouteRecord>
        .ok_or_else(AppError::not_found) // None -> Err(not found); Some(r) -> Ok(r)
}
```

Other helpers you will see:

| Code | Meaning |
| --- | --- |
| `x.unwrap_or("")` | use the value, or a default |
| `x.ok()` | `Result` → `Option`, discarding the error |
| `cond.then_some(v)` | `Some(v)` if `cond` is true, else `None` (used for `gain_m` in `gpx.rs`) |
| `x.is_some_and(\|u\| ...)` | `true` only if `Some` and the check passes |
| `let Some(b) = ... else { continue; };` | unwrap or leave this branch (`backfill_route_boxes`) |
| `if let Some(cookie) = viewer.set_cookie { ... }` | run the block only for `Some` |
| `.unwrap()` / `.expect("msg")` | crash if `None`/`Err`. Fine in tests; in app code only when failure is truly impossible (`random_token`) |

### 4.6 Traits — Book ch. 10.2

A trait is a set of methods a type promises to have, similar to an interface. Axum needs every handler's return value to implement `IntoResponse`. Our handlers return `Result<Response, AppError>`, so `web.rs` implements the trait for its error type:

```rust
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let escaped = escape(&self.1);
        (self.0, Html(format!("...<h1>{escaped}</h1>..."))).into_response()
    }
}
```

That single `impl` is why `?` in any handler produces a proper error page with the right status code.

`impl Trait` in argument position means "any type that implements this":

```rust
fn bad(msg: impl Into<String>) -> Self   // accepts &str or String
fn internal(error: impl std::fmt::Display) -> Self   // accepts any printable error
```

### 4.7 Generics and closures — Book ch. 10.1, 13.1

`nearby::rank` ([nearby.rs:133](../src/domain/nearby.rs#L133)) works with any item type `T`, so tests can rank simple values while the web layer ranks `RouteRecord`s:

```rust
pub fn rank<T>(items: Vec<T>, geometry: impl Fn(&T) -> &str, near: Near) -> Vec<(T, f64)>
```

`geometry` is a closure, an inline function the caller passes in, like `|r| r.geometry_json.as_str()`. Closures are written `|args| body` and can use variables from the surrounding code.

### 4.8 Iterators — Book ch. 13.2

Rust code tends to chain iterator adapters instead of writing index loops. From `gpx::parse`:

```rust
let count: usize = segments.iter().map(Vec::len).sum();

for pair in seg.windows(2) {          // overlapping pairs: [p0,p1], [p1,p2], ...
    distance_m += haversine(&pair[0], &pair[1]);
}
```

- `.iter()` borrows each item; `.into_iter()` takes ownership of them (as `rank` does).
- `.filter`, `.map` and `.filter_map` are lazy. Nothing runs until something consumes them, such as `.collect()`, `.sum()` or a `for` loop.
- `.collect()` builds whatever type you ask for. A neat trick in `gpx::parse`: collecting an iterator of `Option<T>` into `Option<Vec<T>>` gives `None` if *any* item was `None`:

```rust
let times: Option<Vec<_>> = segments[0].iter().map(|p| p.time).collect();
```

### 4.9 Lifetimes — Book ch. 10.3

A lifetime (`'a`) tells the compiler how long a borrow stays valid. Usually it is inferred; you only write one when a function returns a borrow and the compiler cannot tell which input it came from:

```rust
// web.rs — the returned &str points into `form`, not into `name`
fn val<'a>(form: &'a HashMap<String, String>, name: &str) -> &'a str {
    form.get(name).map(String::as_str).unwrap_or("")
}
```

You will also see `'static`, which means "lives for the whole program": string literals, and `Handlebars<'static>` in `AppState`. `Node<'_, '_>` in `gpx.rs` means "a borrowed XML node; infer the lifetimes". The parsed `Point`s are owned copies, so they outlive the XML document they came from.

### 4.10 `async`, `.await` and Tokio — [Async Book](https://rust-lang.github.io/async-book/)

Waiting on the network, disk or database should not block a thread. An `async fn` returns a *future*, and `.await` pauses that function until the result is ready, letting other requests run in the meantime.

```rust
#[tokio::main]                 // starts the Tokio runtime that drives futures
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    presentation::web::run().await
}
```

- A future does nothing until it is `.await`ed.
- Use async versions of blocking APIs inside handlers: `tokio::fs::read`, not `std::fs::read`.
- `Box<dyn std::error::Error>` means "any error type, stored on the heap". It is convenient in `main`/`run`, where we only need to report the error and stop.

### 4.11 Shared state with `Arc` and Axum extractors

Many requests run at once, and all of them need the database pool and templates. `Arc` (atomically reference-counted pointer) lets them share one `AppState` cheaply. Cloning an `Arc` copies a pointer, not the data:

```rust
type Shared = Arc<AppState>;
let state = Arc::new(AppState { pool, templates, ... });
```

Handler arguments are **extractors**. Axum reads the type and fills it from the request:

```rust
async fn route_detail(
    State(state): State<Shared>,   // the shared AppState
    headers: HeaderMap,            // request headers (cookies)
    Path(slug): Path<String>,      // the {slug} part of the URL
) -> WebResult
```

`State(state)` is a *pattern*. It unpacks the wrapper right in the argument list. Others used here: `Query<ExploreQuery>` (URL `?a=b`, deserialized by serde), `Form<HashMap<..>>` and `Multipart` (file uploads).

### 4.12 Macros — Book "Advanced Features: Macros"

Anything ending in `!` is a macro: code that writes code at compile time.

| Macro | What it does here |
| --- | --- |
| `format!("{}-{}", a, b)` / `format!("{name}")` | build a `String`; `{name}` reads a variable directly |
| `json!({"title": title})` | build a `serde_json::Value` for the template |
| `include_str!("../../templates/page.hbs")` | embed a file's text into the binary at compile time |
| `sqlx::migrate!()` | embed `migrations/` and apply them on startup |
| `assert!`, `assert_eq!` | checks in tests |
| `tracing::info!(updated, "...")` | structured log line |

### 4.13 Statics and lazy initialization

[identity/email.rs](../src/identity/email.rs) builds the disposable-domain set once, the first time it is used, then reuses it:

```rust
static DISPOSABLE: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("../../resources/disposable-domains.txt").lines() /* ... */ .collect()
});
```

The strings are `&'static str` because they point into text embedded in the binary.

### 4.14 Database access with SQLx

```rust
sqlx::query_as("SELECT * FROM routes WHERE slug=?")
    .bind(slug)               // fills the ? safely: never format user input into SQL
    .fetch_optional(pool)     // Option<RouteRecord>: zero or one row
    .await
```

`fetch_one`, `fetch_all` and `execute` cover the other cases. `query_scalar` returns a single column. Transactions look like this:

```rust
let mut tx = state.pool.begin().await?;
sqlx::query("...").execute(&mut *tx).await?;  // &mut *tx: borrow the connection inside the transaction
tx.commit().await?;                           // dropping tx without commit rolls back
```

### 4.15 Tests — Book ch. 11

Tests live at the bottom of the file they test:

```rust
#[cfg(test)]          // compile this module only for `cargo test`
mod tests {
    use super::*;     // bring the parent module's items (even private ones) into scope

    #[test]
    fn private_routes_need_owner_or_admin() {
        assert!(!may_view(Visibility::Private, "owner", None, false));
    }
}
```

- Domain tests (`route.rs`, `gpx.rs`, `nearby.rs`) are plain `#[test]` functions.
- HTTP tests in `web.rs` are `#[tokio::test] async fn`. `setup()` creates a temporary database, and `app.oneshot(request)` sends one fake request through the real router without opening a port.
- Run one test by name: `cargo test private_routes`. Show `println!` output: `cargo test -- --nocapture`.

## 5. Reading compiler errors

The compiler is strict but explains itself. Read the first error first; later ones are often caused by it.

- **"borrow of moved value"**: you gave ownership away (passed a `String` by value, or used `into_iter`) and then used it again. Pass `&value` instead, or `.clone()` if you really need two copies.
- **"cannot borrow as mutable"**: add `mut` to the `let`, or check that nothing else is borrowing it at the same time.
- **"mismatched types: expected `&str`, found `String`"**: add `&` (or `.as_str()`). The reverse is fixed with `.to_owned()`.
- **"the `?` operator can only be used..."**: the function's return type must be a `Result`/`Option` whose error type the `?` can convert into. In handlers, use `.map_err(AppError::internal)?` or `.map_err(AppError::bad)?`.
- **"missing match arm"**: you added an enum variant; handle it wherever the compiler points.

Run `rustc --explain E0382` (any error code) for a longer explanation.

## 6. Practice exercises

Each one is small, touches one layer, and has a test to prove it works.

1. **Domain:** add a test in `route.rs` for `slugify` (for example, `"Hello, World!"` keeps only letters, digits and single dashes). Run `cargo test slugify`.
2. **Enum + match:** add a `"cyclocross"` cycling type to `RouteDraft::from_fields`, then find every other place that lists cycling types (`grep -rn gravel src templates`).
3. **Option:** read `segments_do_not_join_and_missing_metrics_are_absent` in `gpx.rs`, then write a test showing that a single-segment GPX whose `<time>` values go *backwards* gives `duration_s == None`.
4. **Iterators:** rewrite the `for pair in seg.windows(2)` distance loop in `gpx::parse` as `.map(...).sum()` and check that the tests still pass. (Then decide which version reads better.)
5. **HTTP:** copy the shape of `public_pages_render_and_route_privacy_is_enforced` in `web.rs` to test that `GET /does-not-exist` returns `404`.

Before committing, run `cargo fmt`, `cargo check` and `cargo test`, as listed in [AGENTS.md](../AGENTS.md).
