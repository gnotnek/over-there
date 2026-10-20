# Routes Near Me Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Explore opens with a map and, when the rider shares a location, the up to 20 nearest public routes within 50 km, nearest first.

**Architecture:** A new domain module `src/domain/nearby.rs` owns location parsing, bounding boxes and nearest-first ranking against the stored public geometry. Routes gain four bounding-box columns written at upload and backfilled at startup; Explore prefilters by box in SQL and ranks in Rust. The page stays server-rendered; a small `static/explore.js` asks for location, reloads with `near=`, and draws pins with the MapLibre setup the route page already uses.

**Tech Stack:** Rust (Axum, SQLx/SQLite, serde_json), Handlebars, vanilla JS, MapLibre GL 5.24.0 from unpkg.

**Spec:** [docs/specs/2026-10-20-nearby-routes-design.md](../specs/2026-10-20-nearby-routes-design.md)

## Global Constraints

- Radius `50_000.0` m, at most `20` results, nearest first, no pagination in nearby mode.
- Distances use only `routes.geometry_json` (public, endpoint-trimmed, simplified). Never the original GPX.
- Rider location rounded to 2 decimals in the browser and again on the server; never stored; request logs record the path only.
- Only `visibility='public'` routes appear. Existing filters (q, region, cycling_type, difficulty, min/max km) still apply.
- Invalid or absent `near` behaves exactly like today's Explore.
- No new JavaScript library or second map library; reuse `maplibre-gl@5.24.0` and `MAP_STYLE_URL`.
- Lime is a fill or a color on black only (Phase 1 rule); colors come from `:root` tokens.
- Branch `feat/nearby-routes`, cut from `feat/ui-refresh` (PR #1). `cargo fmt --check`, `cargo check`, `cargo test` pass after every task.

## Review Focus

- A route whose geometry crosses the antimeridian or sits near a pole: the search box must not wrongly exclude it.
- `near` with surrounding spaces, `+` signs, or extra precision (`38.70712,-9.13551`) must parse and round, not fall back.
- Location denied, unavailable or timing out: the page stays usable, explains why in one line, and does not loop or reload.
- Rider chooses "Newest" after a nearby search: the page must not auto-relocate back to nearest.
- Many pins (20) and one pin: the map fits them without zooming past street level (`maxZoom` 14) and with the rider marker included.

Lines 1–2 get tests in Task 1; lines 3–5 are checked in Task 4's visual step.

---

### Task 0: Branch and plan

- [ ] **Step 1:** Confirm `git branch --show-current` is `feat/nearby-routes` and its parent is `feat/ui-refresh` (`git merge-base --is-ancestor feat/ui-refresh HEAD`).
- [ ] **Step 2: Commit** the updated spec and this plan: `docs: plan routes near me`.

### Task 1: Domain module `nearby`

**Files:**
- Create: `src/domain/nearby.rs` (with `#[cfg(test)] mod tests`)
- Modify: `src/domain/mod.rs` (add `pub mod nearby;`)

**Interfaces:**
- Produces:
  - `pub const RADIUS_M: f64 = 50_000.0;` `pub const LIMIT: usize = 20;`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Near { pub lat: f64, pub lon: f64 }`
  - `pub fn parse_near(raw: &str) -> Option<Near>`: `"lat,lon"`, trims whitespace around each part, both finite, lat in −90..=90, lon in −180..=180, exactly two parts; result rounded to 2 decimals (`(v * 100.0).round() / 100.0`).
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct BBox { pub min_lat: f64, pub max_lat: f64, pub min_lon: f64, pub max_lon: f64 }`
  - `pub fn bbox(geometry_json: &str) -> Option<BBox>`: over every `[lon, lat]` of a GeoJSON `MultiLineString` (`coordinates`); `None` when there are no points or the JSON is invalid.
  - `pub fn search_box(near: Near, radius_m: f64) -> BBox`: lat ± `radius_m / 111_320.0` clamped to ±90; lon ± `radius_m / (111_320.0 * cos(lat))`; when the lat box reaches a pole or the lon box crosses ±180, lon becomes −180..180 (`shortcut:` comment: whole longitude band near poles/antimeridian, fine at this scale).
  - `pub fn distance_m(geometry_json: &str, near: Near) -> Option<f64>`: minimum `gpx::haversine` from `near` to any vertex; `None` without points.
  - `pub fn first_point(geometry_json: &str) -> Option<[f64; 2]>`: first `[lon, lat]` of the first line.
  - `pub fn rank<T>(items: Vec<T>, geometry: impl Fn(&T) -> &str, near: Near) -> Vec<(T, f64)>`: keep items with `distance_m <= RADIUS_M`, sort ascending by distance (`total_cmp`), truncate to `LIMIT`.

- [ ] **Step 1: Write the failing tests** in `nearby.rs` (synthetic coordinates only):
  - `parse_near_accepts_trims_and_rounds`: `parse_near(" 38.70712 , -9.13551 ")` → `Near { lat: 38.71, lon: -9.14 }`; `parse_near("-90,180")` is `Some`.
  - `parse_near_rejects_bad_input`: `None` for `""`, `"1"`, `"1,2,3"`, `"91,0"`, `"0,181"`, `"NaN,1"`, `"inf,0"`, `"a,b"`.
  - `bbox_covers_every_segment`: geometry with two lines `[[0,0],[1,1]]` and `[[-2,3],[0.5,-1]]` → `BBox { min_lat: -1, max_lat: 3, min_lon: -2, max_lon: 1 }`; `bbox("{}")` and empty coordinates → `None`.
  - `search_box_contains_points_at_the_radius`: for `Near { lat: 38.7, lon: -9.1 }`, points 49.9 km due north, south, east and west (built by offsetting degrees and checked with `gpx::haversine` < 50 km) lie inside `search_box(near, RADIUS_M)`.
  - `search_box_widens_at_poles_and_antimeridian`: near `(89.9, 0)` and `(0, 179.9)` → `min_lon == -180.0 && max_lon == 180.0`.
  - `distance_ignores_hidden_endpoints`: a straight synthetic segment starting at the rider; `gpx::geojson(&gpx::public_segments(&[segment], 1000.0)?)` → `distance_m(.., rider)` ≥ 900 m, while the untrimmed geometry gives < 1 m.
  - `rank_filters_sorts_and_caps`: 30 items at 1..=30 km plus 3 at 60 km → 20 results, distances ascending, all ≤ `RADIUS_M`.
- [ ] **Step 2:** `cargo test nearby` → FAIL (module empty / functions missing).
- [ ] **Step 3:** Implement the Interfaces above in `nearby.rs`; parse geometry with `serde_json::Value` like `preview_svg` does.
- [ ] **Step 4:** `cargo test nearby` → PASS; `cargo test` → all PASS.
- [ ] **Step 5: Commit** `feat(domain): rank public routes by distance from a rounded location`

### Task 2: Store route bounding boxes

**Files:**
- Create: `migrations/20261021000000_route_bbox.sql`
- Modify: `src/infrastructure/routes.rs`, `src/presentation/web.rs` (upload INSERT near line 930; `run()` after `sqlx::migrate!()` near line 290)
- Test: `src/presentation/web.rs` (`mod tests`)

**Interfaces:**
- Consumes: `nearby::bbox`.
- Produces:
  - Columns `min_lat REAL, max_lat REAL, min_lon REAL, max_lon REAL` (nullable) and `CREATE INDEX routes_bbox_idx ON routes(visibility, min_lat, max_lat);`
  - `pub async fn backfill_route_boxes(pool: &SqlitePool) -> Result<u64, sqlx::Error>` in `src/infrastructure/routes.rs`: selects `id, geometry_json` where `min_lat IS NULL`, writes `nearby::bbox`, returns rows updated; rows whose geometry has no points stay NULL.
  - Test helpers in `mod tests`, extracted from `public_pages_render_and_route_privacy_is_enforced` (which then calls them): `async fn signed_in_rider(app: &Router, state: &Shared) -> String` (registers `test_rider`, verifies email, returns session cookie) and `async fn upload_route(app: &Router, session: &str, title: &str, gpx: &str, visibility: &str, difficulty: &str) -> String` (returns the `Location` path).

- [ ] **Step 1: Write the failing tests:**
  - `upload_stores_public_geometry_box`: upload a public synthetic route; `SELECT min_lat,max_lat,min_lon,max_lon FROM routes` equals `nearby::bbox(geometry_json)`.
  - `backfill_fills_missing_route_boxes`: after upload, `UPDATE routes SET min_lat=NULL,max_lat=NULL,min_lon=NULL,max_lon=NULL`; `backfill_route_boxes` returns 1; columns equal `nearby::bbox(geometry_json)`; a second call returns 0.
- [ ] **Step 2:** `cargo test route_box` → FAIL (columns / function missing).
- [ ] **Step 3:** Add the migration, write the four columns in the upload INSERT, implement `backfill_route_boxes`, call it in `run()` right after migrations with `tracing::info!(updated, "route boxes backfilled")` when `updated > 0`.
- [ ] **Step 4:** `cargo test` → all PASS (existing privacy test still passes through the extracted helpers).
- [ ] **Step 5: Commit** `feat(routes): store public geometry bounding boxes`

### Task 3: Nearby mode in the Explore handler

**Files:** Modify `src/presentation/web.rs` (`ExploreQuery`, `explore`, `router`'s `TraceLayer`). Test: `mod tests`.

**Interfaces:**
- Consumes: `nearby::{parse_near, search_box, rank, first_point, RADIUS_M}`, Task 2 helpers.
- Produces (template context, used by Task 4):
  - `ExploreQuery.near: Option<String>`.
  - Nearby mode ⇔ `parse_near` succeeds and `sort` is empty or `"nearest"`.
  - `data["near"]`: `"lat,lon"` string of the rounded location when it parsed (in either mode, so the form can carry it), else absent.
  - `data["nearby"]`: bool, nearby mode.
  - `data["filters"]["nearest"]`: bool for the sort `<option>`.
  - Each route JSON in nearby mode gains `"away_km": "{:.1}"`.
  - `data["pins_json"]`: JSON string `[{"slug","title","lon","lat"}]` for the routes on the page (both modes), from `first_point`.
  - Nearby mode: `count` = results length, `has_prev`/`has_next` false.

- [ ] **Step 1: Write the failing tests** (three public routes: A ~1 km from `near`, B ~10 km, C ~200 km; plus an unlisted route ~1 km; synthetic coordinates around `(10.00, 20.00)`):
  - `explore_near_lists_public_routes_within_radius_nearest_first`: `GET /explore?near=10.00,20.00` → body has A's title before B's, no C, no unlisted title, contains `km away`.
  - `explore_near_keeps_filters`: same with `&difficulty=hard` where only B is hard → B only.
  - `explore_ignores_invalid_near`: `near=999,0` → all three public titles, no `km away`.
  - `explore_other_sort_ignores_near`: `near=10.00,20.00&sort=newest` → all three public titles, no `km away`, and the hidden `near` value `10,20` is in the form.
- [ ] **Step 2:** `cargo test explore_` → FAIL.
- [ ] **Step 3:** Implement: nearby branch runs the existing filtered public query without ORDER/LIMIT plus `AND min_lat<=? AND max_lat>=? AND min_lon<=? AND max_lon>=?` bound from `search_box(near, RADIUS_M)` (box overlap), then `nearby::rank(routes, |r| &r.geometry_json, near)`. Keep the current branch unchanged otherwise.
- [ ] **Step 4:** Replace `TraceLayer::new_for_http()` with `.make_span_with(|req: &Request<_>| tracing::info_span!("request", method = %req.method(), path = %req.uri().path()))` so query strings are never logged.
- [ ] **Step 5:** `cargo test` → all PASS. **Commit** `feat(explore): nearest public routes within 50 km`

### Task 4: Explore map page

**Files:** Modify `templates/page.hbs` (head maplibre CSS, explore block), `static/style.css` (explore section, phone block). Create `static/explore.js`.

**Interfaces:** Consumes Task 3 context keys only. DOM contract between template and `explore.js`:
- `#explore-map[data-style][data-pins][data-near]` (`data-near` empty when absent), containing `.map-fallback`.
- `#near-me` button (text "Show routes near me"), `#near-status` (`role="status"`).
- Route cards carry `data-slug`.
- The filter form carries `<input type="hidden" name="near">` when `near` is set; the sort select gets `<option value="nearest">Nearest</option>` (selected when `nearby`) only when `near` is set.

- [ ] **Step 1: Template.** Load `maplibre-gl@5.24.0` CSS when `page_explore` too. Replace the photo `.page-heading` on Explore with a plain heading row (title, count, and in nearby mode "Within 50 km of you, nearest first"). Below the filters: `.explore-split` with the map card (`#explore-map`, button, status) and the route grid. Cards show `{{away_km}} km away` when present. Nearby empty state: "No public routes within 50 km." with a link to `/explore?sort=newest`. Scripts at the end of the Explore block: maplibre JS (defer) then `/static/explore.js` (defer).
- [ ] **Step 2: `static/explore.js`** (IIFE like `route.js`, under 120 lines):
  - `locate()`: `navigator.geolocation.getCurrentPosition` with `{timeout: 10000, maximumAge: 600000}`; on success set `near` to `lat.toFixed(2),lon.toFixed(2)`, `sort=nearest`, delete `page`, `location.assign`. On error write one line to `#near-status`: denied → "Location is off, so routes are shown newest first."; otherwise → "Couldn't find your location. Showing newest routes." Disable the button while waiting.
  - Auto-locate only when `location.search === ''` and `navigator.permissions?.query({name: 'geolocation'})` resolves `granted`; otherwise wait for the button. Hide the button when `data-near` is set.
  - Map: if `!window.maplibregl` show the fallback text and stop. Otherwise create the map with `data-style`, add a `.map-point` marker per pin (title = route title) and a `.map-you` marker at `data-near`; `fitBounds` over pins plus rider with `{padding: 55, maxZoom: 14, duration: 0}`; no pins and no rider → keep the default view. Clicking a pin scrolls its card into view and toggles `.is-active` on it; hovering a card toggles `.is-active` on its pin.
- [ ] **Step 3: CSS.** `.explore-split` grid `minmax(0,1.1fr) minmax(0,1fr)`, map card `--r-sheet`, sticky at `top:12px`, height `min(70vh,640px)`; route grid inside it 2 columns, 1 under 1100px; phone: map first at 360px tall, not sticky. `.map-you` black dot with lime ring; `.map-point.is-active` and `.route-card.is-active` use `--ring`. `#near-me` positioned over the map top-left as a `.button-light`; `#near-status` muted line under the heading. All colors from tokens (the guard test enforces it).
- [ ] **Step 4: Visual check** on a preview server started with its own scratch `DATABASE_URL`/`STORAGE_DIR` on port 3100 (stop it later by its PID only, never by name). Seed synthetic routes around one point. Check at 1280px and 375px: bare `/explore` (button visible, newest list, pins for listed routes); `/explore?near=…` (nearest first, distances, rider marker, map fits); a `near` with nothing within 50 km (empty state); `?near=…&sort=newest` (no auto-relocate, Nearest option available); denied location (emulate by overriding `navigator.geolocation.getCurrentPosition` in the console to call the error callback with code 1) shows the one-line message. Pass when Review Focus lines 3–5 hold and there is no horizontal scroll at 375px.
- [ ] **Step 5:** `cargo test` → PASS (template still renders in web tests). **Commit** `feat(explore): map with nearby route pins and location button`

### Task 5: Docs

- [ ] **Step 1:** `docs/architecture.md`: in "Data and visibility", one sentence on the public-geometry bounding box columns and that nearby distance uses only public geometry; in "Primary flows", Explore opens on a map with nearby routes.
- [ ] **Step 2:** `docs/roadmap.md` Phase 2 → `(done)`; spec status → done.
- [ ] **Step 3:** `cargo fmt --check && cargo check && cargo test` → PASS. **Commit** `docs: mark routes near me done`
