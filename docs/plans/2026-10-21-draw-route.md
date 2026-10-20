# Plan a Route on the Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Riders create a route by clicking points on a map (lines follow roads via BRouter in the browser), can later edit that line, and climbing is counted correctly for every route.

**Architecture:** The browser routes legs with BRouter and posts the whole line plus a small "plan" (snapped points, their indexes, straight flags) as two multipart file parts. The server checks them in `domain::gpx`, builds a GPX, and runs the existing `upload` path unchanged; the plan goes in a new owner-only column. A line editor reuses the same script and recomputes everything derived from the GPX. A one-time startup task recomputes climbing for saved routes with the corrected rule.

**Tech Stack:** Rust (Axum, SQLx/SQLite, serde_json), Handlebars, vanilla JS, MapLibre GL 5.24.0 from unpkg, BRouter HTTP API (browser only).

**Spec:** [docs/specs/2026-10-21-draw-route-design.md](../specs/2026-10-21-draw-route-design.md)

## Global Constraints

- Plan: 2–100 points. Line: 2–100,000 points, finite, lat −90..90, lon −180..180.
- Routing request timeout 10 s; on failure the leg is straight and dashed with the status "Couldn't follow roads here, so this part is a straight line."
- Profiles: road → `fastbike`, gravel → `gravel`, mountain → `mtb`, everything else → `trekking`.
- `ROUTING_URL` defaults to `https://brouter.de/brouter`. Credit line: "Routing by BRouter. The points you place are sent to <host>."
- Climbing: reference-height rule with a 3 m threshold, per segment, for gain and loss; same rule in `draw.js`.
- `plan_json` is owner-only: never in route detail, geometry, downloads or any visitor JSON.
- Drawn routes have no duration; line edits keep slug, details, visibility, endpoint hiding, photos, download count.
- No new JS library; MapLibre 5.24.0 and `MAP_STYLE_URL` as on other pages; lime casing + black line, straight legs dashed; colors from `:root` tokens.
- `cargo fmt --check`, `cargo check`, `cargo test` pass after every task. Synthetic coordinates only.

## Review Focus

- A `line`/`plan` pair where the plan's point does not match `line[at[k]]` exactly (tampered or float-formatted differently): 400, not a corrupt plan saved.
- A drawn route posted twice with the same line: the duplicate check rejects it like a re-uploaded file.
- Saving an unchanged line in the editor: must not be rejected as a duplicate of itself.
- A saved route whose original GPX file is missing at startup: recompute skips it and still marks the task done.
- A route with hidden endpoints: the line editor (owner) gets the full plan, but the plan's start never appears on the public route page.

Lines 1 and 4 get tests in Tasks 2 and 1; lines 2, 3 and 5 get tests in Tasks 3 and 4.

---

### Task 1: Correct climbing rule and recompute saved routes

**Files:**
- Modify: `src/domain/gpx.rs` (gain/loss loop in `parse`, tests)
- Create: `migrations/20261022000000_pending_tasks.sql`
- Modify: `src/presentation/web.rs` (`run`, new `recompute_climbing`, tests)

**Interfaces:**
- Produces: `async fn recompute_climbing(pool: &SqlitePool, storage: &FilePath) -> Result<Option<u64>, sqlx::Error>` in `web.rs` (None when not pending).

- [ ] **Step 1: Failing domain tests** in `gpx.rs`:
  - `gentle_climb_counts_in_full`: 41 points 0.0005° apart, `ele` rising 1 m each (0..=40) → `gain_m == Some(40.0)`, `loss_m == Some(0.0)`.
  - `small_noise_is_not_climbing`: 20 points alternating `ele` 10 and 12 → `gain_m == Some(0.0)`.
- [ ] **Step 2:** `cargo test gpx::` → the gentle climb test fails (old rule gives 0).
- [ ] **Step 3: Implement** the rule from the spec inside `parse`: per segment, `reference` = first `Some(ele)`; on a later point with ele, if `ele - reference > 3.0` add to gain and set reference; if `reference - ele > 3.0` add to loss and set reference. `elevation_points > 0` still decides `Some`/`None` (count pairs with both elevations, as now).
- [ ] **Step 4:** Migration: `CREATE TABLE pending_tasks(name TEXT PRIMARY KEY); INSERT INTO pending_tasks VALUES ('recompute_climbing');` with a one-line comment.
- [ ] **Step 5: Failing web test** `startup_recomputes_climbing_once`: upload `TAIPEI_GPX`, `UPDATE routes SET elevation_gain_m=0`, insert a second route row pointing at a missing `gpx_key` (copy the first row with a new id/slug/gpx_key/sha). `recompute_climbing` returns `Some(1)`, gain is back to 35, the broken row is untouched, `pending_tasks` is empty, and a second call returns `None`.
- [ ] **Step 6: Implement** `recompute_climbing` next to `strip_redacted_times`: return `None` unless the task row exists; for each `(id, gpx_key)` read and `gpx::parse`, warn-and-skip on error, `UPDATE routes SET elevation_gain_m=?, elevation_loss_m=?`; delete the task row; return the count. Call it in `run` after `strip_redacted_times`, logging `updated` when `Some(n)`.
- [ ] **Step 7:** `cargo test` passes.
- [ ] **Step 8: Commit** `fix(gpx): count gradual climbs and recompute saved routes`.

### Task 2: Drawn line to GPX, with plan check

**Files:**
- Modify: `src/domain/gpx.rs`

**Interfaces:**
- Produces:
  - `pub fn line_gpx(line: &[Point]) -> Vec<u8>`: one `<trk><trkseg>`, `<ele>` when present, no `<time>`, `<name>Planned route</name>`.
  - `#[derive(Deserialize, Serialize)] pub struct Plan { pub points: Vec<[f64; 2]>, pub at: Vec<usize>, pub straight: Vec<bool> }`
  - `pub fn drawn_route(line_json: &[u8], plan_json: &[u8]) -> Result<(Vec<u8>, String), String>`: parses `line_json` as `Vec<Vec<f64>>` (each `[lon, lat]` or `[lon, lat, ele]`), checks it and the plan, returns `(line_gpx bytes, plan re-serialised as JSON)`.

- [ ] **Step 1: Failing tests:**
  - `line_gpx_round_trips`: a 3-point line with elevation and one without; `parse(&line_gpx(..))` keeps point count, lat/lon, `ele` (`Some`/`None`), and `distance_m` equal to the sum of `haversine` pairs.
  - `drawn_route_accepts_a_valid_plan`: line `[[121,25,5],[121,25.01,9],[121,25.02,30]]`, plan `{"points":[[121,25],[121,25.02]],"at":[0,2],"straight":[false]}` → `Ok`, and the GPX parses to 3 points.
  - `drawn_route_rejects_broken_plans`: each is `Err` — 1 plan point; 101 plan points; `at` not starting at 0; `at` not ending at the last index; `at` not strictly increasing; `straight` length ≠ points − 1; a point ≠ `line[at[k]]`; a line point with lat 91; a line entry with 1 or 4 numbers; invalid JSON.
- [ ] **Step 2:** `cargo test gpx::` fails to compile (functions missing).
- [ ] **Step 3: Implement** the three items above. Line limits come from Global Constraints; reuse the coordinate check wording from `parse_points` ("Point is outside valid coordinates"). Error strings are user-facing (shown on the 400 page), e.g. "The drawn route is incomplete; draw it again".
- [ ] **Step 4:** `cargo test` passes.
- [ ] **Step 5: Commit** `feat(gpx): build a GPX from a drawn line`.

### Task 3: Create drawn routes through upload; `/routes/new`

**Files:**
- Create: `migrations/20261022000001_route_plan.sql` (`ALTER TABLE routes ADD COLUMN plan_json TEXT;` + comment "Owner-only: the points a rider placed when drawing the route. Null for uploaded GPX.")
- Modify: `src/infrastructure/routes.rs` (`RouteRecord.plan_json: Option<String>`)
- Modify: `src/presentation/web.rs` (`AppState.routing_url`, `run`, test `setup`, `upload`, new `draw_form`, router)
- Modify: `templates/page.hbs` (upload section), `.env.example`

**Interfaces:**
- Consumes: `gpx::drawn_route` (Task 2).
- Produces: `GET /routes/new` → `draw_form`, rendering page `upload` with `"draw": true` and `routing_url`; `upload` accepts `line` + `plan` file parts and stores `plan_json`; `AppState.routing_url: String`.

- [ ] **Step 1: Failing web tests** (helper `draw_route(app, session, title, line, plan) -> String` modelled on `upload_route`, posting `line`/`plan` as file parts named `line.json`/`plan.json`):
  - `drawn_route_is_published_and_renders`: GET `/routes/new` is 200 and contains `id="draw-map"`; posting the Task 2 valid line/plan as public redirects to `/routes/…`; the detail page contains the title, `2.2` (km) and `elevation-chart`; `plan_json` is stored.
  - `drawn_route_needs_line_and_plan`: line without plan → 400; neither gpx nor line → 400; same line/plan twice → 400 ("You already uploaded…").
  - `hidden_plan_stays_private`: drawn route with `hide_endpoints=250`, signed-out GET of the detail page and `/geometry` contains neither `"points"` nor the start coordinate `121,25` / `[121.0,25.0]`.
- [ ] **Step 2:** `cargo test` → new tests fail.
- [ ] **Step 3: Implement.** In `upload`, replace the `gpx` lookup with: if `line` or `plan` present → both required ("Draw at least two points" otherwise) → `gpx::drawn_route` → `(raw, Some(plan))`; else the `gpx` file as now with `plan = None`. Everything after stays as is; add `plan_json` to the INSERT. `draw_form` mirrors `upload_form`. Read `ROUTING_URL` in `run` (default from Global Constraints), add to `AppState`, `setup()` and the config log line; add a commented `ROUTING_URL=` to `.env.example`.
- [ ] **Step 4: Template.** In the upload section: the Share form gets "No GPX file? <a href="/routes/new">Draw it on the map</a>" under the GPX field. With `draw`, the first section shows `<div class="draw-map" id="draw-map" data-style data-routing>` with `.map-fallback`, controls (`#draw-undo`, `#draw-clear`, `#draw-follow` checkbox "Follow roads" checked, `#draw-locate` "Center on me"), `#draw-stats`, `#draw-status` (role=status), the credit line, and hidden `<input type="file" name="line" hidden>` / `name="plan"`; the submit button gets `id="draw-submit" disabled`. Page title "Draw a route", heading "Draw a route". Load MapLibre CSS/JS and `/static/draw.js` when `draw` or `page_line`.
- [ ] **Step 5:** `cargo test` passes.
- [ ] **Step 6: Commit** `feat(routes): create a route from a drawn line`.

### Task 4: Line editor

**Files:**
- Modify: `src/presentation/web.rs` (`line_form`, `save_line`, router, `edit_form` context, tests)
- Modify: `templates/page.hbs` (edit section link, new `page_line` section)

**Interfaces:**
- Consumes: `gpx::drawn_route`, `RouteRecord.plan_json`, `write_asset`, `remove_assets`.
- Produces: `GET /routes/{slug}/line` → `line_form`, `POST /routes/{slug}/line` (multipart `csrf`, `line`, `plan`) → `save_line`.

- [ ] **Step 1: Failing web tests:**
  - `owner_redraws_line`: draw a route; GET `/routes/{slug}/line` as owner is 200 and contains `data-plan`; POST a longer line (append `[121,25.04,12]`, plan points/at/straight updated) → redirect to the same slug; `distance_m` grew, `gpx_key` changed and the old file is gone; saving the identical line again also succeeds (no self-duplicate).
  - `line_editor_access`: another user → 403 on GET and POST; an uploaded (`TAIPEI_GPX`) route → 404 on GET; the uploaded route's edit page lacks "Edit the line" while a drawn route's edit page has it.
- [ ] **Step 2:** `cargo test` → fails.
- [ ] **Step 3: Implement.** `line_form`: `require_user`, `get_route`, `can_edit` else 403, `plan_json` None → 404; render page `line` with `route`, `routing_url`, `profile` (BRouter profile from the route's cycling type, Global Constraints), `plan_json`, and the line as JSON from re-parsing the stored original GPX (`[[lon,lat,ele?],...]` from `parse(..).segments[0]`). `save_line`: same checks plus `csrf` (from `multipart_values`); `drawn_route`; parse; duplicate check with `AND id<>?`; `public_segments` with the route's `hide_endpoints_m`; write new GPX (+ redacted when hiding); one `UPDATE` of `gpx_key, public_gpx_key, gpx_sha256, geometry_json, elevation_json, distance_m, elevation_gain_m, elevation_loss_m, duration_s=NULL, min/max lat/lon, plan_json, updated_at`; on error remove the new files; after success remove the old ones; log `route line updated`; redirect to the route.
- [ ] **Step 4: Template.** Edit page: `{{#if has_plan}}<a href="/routes/{{route.slug}}/line">Edit the line</a>{{/if}}` near "View route" (`edit_form` sets `has_plan`). New `page_line` section: heading "Edit the line", the same map/controls markup as Task 3 with `data-line`, `data-plan` and `data-profile`, a form `method=post action=/routes/{slug}/line enctype=multipart/form-data` with csrf, hidden file inputs and "Save line". Extract the shared map/controls markup to a registered partial `draw_controls` only if Handlebars partial registration stays a one-liner in both `run` and `setup`; otherwise duplicate it.
- [ ] **Step 5:** `cargo test` passes.
- [ ] **Step 6: Commit** `feat(routes): edit the line of a drawn route`.

### Task 5: `static/draw.js` and styles

**Files:**
- Create: `static/draw.js`
- Modify: `static/style.css`

**Interfaces:**
- Consumes: `#draw-map` `data-style`, `data-routing`, optional `data-line` / `data-plan`; `select[name=cycling_type]` (absent on the line editor → use `data-profile` set from the route's cycling type in Task 4's context); controls and hidden inputs from Tasks 3–4.

- [ ] **Step 1: Implement** the behaviour in spec steps 3–10: state is `points` (snapped `[lon,lat]`) and `legs` (`{coords, straight}`); click adds a point and routes the new leg; markers are draggable (re-route the two touching legs) and click-to-remove (re-route the gap); Undo, Clear; max 100 points; `fetch(${routing}?lonlats=a|b&profile=p&alternativeidx=0&format=geojson)` with `AbortSignal.timeout(10000)`, the leg's first and last coordinates become the snapped points; straight legs as a separate dashed layer; live distance/ascent with the server's rules; "Center on me" via geolocation, never sent anywhere; credit host from `new URL(routing).host`; preload from `data-line`/`data-plan` without fetching; on submit build `line` = concatenated legs without duplicated joints and `plan` = `{points, at, straight}`, put both in the hidden inputs with `DataTransfer`, and let the form submit. Submit stays disabled below two points.
- [ ] **Step 2: Styles**: `.draw-map` height like `.route-map`, controls row, dashed-leg colors from tokens, status text using `--muted`.
- [ ] **Step 3: Visual check** in the browser pane at `http://127.0.0.1:3000/routes/new` (signed-in verified test account from `cargo run` logs): draw 3 points, drag one, remove one, toggle "Follow roads", publish, see the route page with elevation chart; open "Edit the line", add a point, save. Phone width (375 px): map first, controls wrap. Screenshot before/after for the PR.
- [ ] **Step 4: Commit** `feat(ui): draw routes on the map`.

### Task 6: Docs and final checks

**Files:** `docs/architecture.md`, `docs/roadmap.md`, `docs/contracts.md`, `docs/data-catalog.md`, `docs/service-catalog.md`, `docs/specs/2026-10-21-draw-route-design.md` (status → done)

- [ ] **Step 1:** Apply every item in the spec's "Docs to update" (including `pending_tasks` in the data catalog and `recompute_climbing` under startup tasks).
- [ ] **Step 2:** `cargo fmt --check && cargo check && cargo test` all pass.
- [ ] **Step 3: Commit** `docs: phase 3 route planner`.
