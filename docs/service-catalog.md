# Service catalog

Every part of Over There that does a job, internal or external: what it is responsible for, what it exposes, what it depends on, and what happens when it fails. Over There is a single binary today, so the internal "services" are Rust modules. If one is ever split into its own process, its entry here is the starting point.

Add an entry when you add a module, a background task, a browser script, or a third-party dependency that runs at request time. For the HTTP surface see [contracts.md](contracts.md); for tables see [data-catalog.md](data-catalog.md); for how the pieces fit see [architecture.md](architecture.md).

## Summary

| Service | Kind | Location | Owns data | Depends on |
| --- | --- | --- | --- | --- |
| [Route policy](#route-policy) | Domain | `src/domain/route.rs` | — | — |
| [GPX processing](#gpx-processing) | Domain | `src/domain/gpx.rs` | — | `roxmltree`, `chrono` |
| [Nearby search](#nearby-search) | Domain | `src/domain/nearby.rs` | — | GPX processing |
| [Authentication](#authentication) | Identity | `src/identity/auth.rs` | `sessions` | SQLite, `ring`, `sha2` |
| [Email validation](#email-validation) | Identity | `src/identity/email.rs` | `resources/disposable-domains.txt` | — |
| [Route repository](#route-repository) | Infrastructure | `src/infrastructure/routes.rs` | `routes` reads, bbox backfill | SQLite, Nearby search |
| [Web application](#web-application) | Presentation | `src/presentation/web.rs` | everything else | all of the above |
| [Browser islands](#browser-islands) | Client | `static/*.js` | — | MapLibre, web endpoints |
| [Startup tasks](#startup-tasks) | Job | `web::run` | `_sqlx_migrations`, `routes` bbox | SQLite |
| [External services](#external-services) | Third party | — | — | — |

## Domain services

Pure functions: no HTTP, no database, no clock except where noted. They are the safest place to add business rules and the cheapest to test.

### Route policy

`src/domain/route.rs`. Decides who may see or change a route and what a valid route form looks like.

| Interface | Contract |
| --- | --- |
| `Visibility::parse(&str) -> Result<Visibility, String>` | Accepts exactly `public`, `unlisted`, `private` |
| `may_view(visibility, owner_id, actor_id, is_admin) -> bool` | Non-private routes are viewable by anyone who has the slug; private needs the owner or an admin |
| `may_edit(owner_id, actor_id, is_admin) -> bool` | Owner or admin |
| `RouteDraft::from_fields(&HashMap) -> Result<RouteDraft, String>` | Trims and validates title, description, region, country, tags, cycling type, difficulty, visibility. The error string is shown to the user |
| `slugify(title, id) -> String` | URL-safe, unique because it ends with the UUID |

Tests: `private_routes_need_owner_or_admin`, `public_and_unlisted_are_link_viewable`.

### GPX processing

`src/domain/gpx.rs`. Turns an uploaded file into route facts and public geometry.

| Interface | Contract |
| --- | --- |
| `parse(&[u8]) -> Result<RouteData, String>` | ≤ 10 MB, UTF-8, `<gpx>` root, no DTD, ≤ 500,000 XML nodes, 2–100,000 points, valid lat/lon. Reads `trk/trkseg/trkpt` and `rte/rtept`. Segments stay separate. Computes distance, gain/loss (3 m deadband), duration |
| `public_segments(&segments, hide_m) -> Result<Vec<Vec<Point>>, String>` | Drops points within `hide_m` of the route's first and last point; errors if nothing is left |
| `geojson(&segments) -> String` | Simplified `MultiLineString` |
| `elevation_profile(&segments) -> Option<Vec<(dist, ele, lon, lat)>>` | `None` without elevation |
| `redacted_gpx(&segments) -> Vec<u8>` | A fresh GPX 1.1 document from the given segments, with elevation and without times |
| `haversine(&Point, &Point) -> f64` | Metres on a 6,371 km sphere |

Tests: `segments_do_not_join_and_missing_metrics_are_absent`, `rejects_dtd_and_bad_coordinates`, `hiding_keeps_original_unmodified`, `hidden_endpoints_are_absent_from_public_elevation`, `redacted_gpx_drops_times_and_keeps_elevation`.

### Nearby search

`src/domain/nearby.rs`. Finds public routes near a rider ([design](specs/2026-10-20-nearby-routes-design.md)).

| Interface | Contract |
| --- | --- |
| `parse_near(&str) -> Option<Near>` | `"lat,lon"`, in range, rounded to 2 decimals; anything else is `None` |
| `bbox(geometry_json) -> Option<BBox>` | Bounding box of a stored geometry |
| `search_box(near, radius_m) -> BBox` | Box guaranteed to contain the search circle (whole longitude band near poles or the antimeridian) |
| `distance_m(geometry_json, near) -> Option<f64>` | Distance to the nearest stored vertex |
| `rank(items, geometry, near) -> Vec<(T, f64)>` | Within `RADIUS_M` (50 km), nearest first, at most `LIMIT` (20) |
| `first_point(geometry_json)` | Map pin position |

Tests: `parse_near_*`, `bbox_covers_every_segment`, `search_box_contains_points_at_the_radius`, `search_box_widens_at_poles_and_antimeridian`, `distance_ignores_hidden_endpoints`, `rank_filters_sorts_and_caps`, `first_point_is_start_of_first_line`.

## Identity services

### Authentication

`src/identity/auth.rs`. Passwords, sessions, cookies and CSRF. Unlike the domain services, it reads Axum headers and queries SQLite.

| Interface | Contract |
| --- | --- |
| `password_hash(&str) -> String` / `verify_password(&str, &str) -> bool` | PBKDF2-HMAC-SHA256, 310,000 iterations, random salt; constant-time verify |
| `random_token() -> String` | 32 random bytes as 64 hex chars |
| `token_hash(&str) -> String` | SHA-256 hex; what the database stores |
| `create_session(pool, user_id)` | Inserts a 30-day session; returns `(token, csrf)` |
| `session(pool, headers) -> Option<(User, csrf)>` | Resolves the `ot_session` cookie to a user |
| `cookie`, `session_cookie`, `csrf_cookie` | Read and build cookies ([contract](contracts.md#cookies)) |
| `check_csrf(headers, submitted, expected, base_url)` | Token match plus same-origin `Origin` header when present |
| `purge_expired(pool) -> Result<Purged, sqlx::Error>` | Deletes expired sessions and verification links and lapsed sign-in counters; returns counts per table |

Tests: `passwords_are_salted_and_verified`, `purge_removes_only_expired_records`; HTTP tests in `web.rs` cover sign-in flows.

### Email validation

`src/identity/email.rs`. `validate(&str) -> Result<(), &'static str>` checks address syntax and rejects disposable domains, including their subdomains. The list is compiled into the binary from `resources/disposable-domains.txt`; refresh it with `scripts/update_disposable_domains.py`. No network call per signup.

Tests: `blocks_known_disposable_domains_and_subdomains`, `rejects_invalid_addresses`.

## Infrastructure services

### Route repository

`src/infrastructure/routes.rs`.

| Interface | Contract |
| --- | --- |
| `RouteRecord` | One `routes` row (`SELECT *`), mapped by column name |
| `find_by_slug(pool, slug) -> Result<Option<RouteRecord>, sqlx::Error>` | No access check; callers must apply route policy |
| `backfill_route_boxes(pool) -> Result<u64, sqlx::Error>` | Fills missing bounding boxes; idempotent |

Most other SQL still lives in the handlers. Move queries here when a second caller needs them, not before.

## Presentation services

### Web application

`src/presentation/web.rs`. The only HTTP adapter: configuration, router, handlers, rendering, file storage and outbound email. Grouped by feature:

| Feature | Handlers | Tables written | Rules used |
| --- | --- | --- | --- |
| Discovery | `home`, `explore`, `profile` | — | Nearby search |
| Route pages and files | `route_detail`, `route_geometry`, `download`, `route_cover`, `route_image`, `avatar` | `routes.download_count` | Route policy |
| Contribution | `upload_form`, `upload`, `edit_form`, `edit_route`, `delete_route`, `dashboard` | `routes`, `route_images`, files | Route policy, GPX processing, Nearby (bbox) |
| Accounts | `register*`, `login*`, `logout`, `verify_email`, `account_*`, `resend_verification`, `delete_account` | `users` (and every cascade from it), `sessions`, `auth_attempts`, `email_verifications`, `email_send_quota`, files | Authentication, Email validation |
| Moderation | `report_route`, `admin`, `admin_contributor`, `admin_report` | `route_reports`, `routes.visibility`, `users.contributor_verified_at` | — |

Shared helpers: `viewer` (who is asking, CSRF token), `require_user`, `csrf`, `get_route`, `can_view`/`can_edit`, `render`, `write_asset`/`serve_image`/`remove_assets`, `optimized_image`, `AppError` (logs every 4xx with its reason, every 5xx with its cause).

When this file becomes hard to navigate, split it along the feature rows above into `presentation/{discovery,routes,accounts,admin}.rs`, keeping helpers in `presentation/web.rs`.

Tests: `#[cfg(test)]` in `web.rs`, driving the real router with `tower::ServiceExt::oneshot` against a temporary database.

### Browser islands

Small scripts, loaded only on the pages that need them. All rendering happens on the server; these add interaction.

| Script | Page | Does | Calls |
| --- | --- | --- | --- |
| `static/app.js` | All | Shows the `?notice=` message as a toast | — |
| `static/route.js` | Route detail | Map with the route line, elevation chart synced to the map, share button | `GET /routes/{slug}/geometry` |
| `static/explore.js` | Explore | Map with pins, "Near me" geolocation (rounded to 2 decimals before navigation) | Navigates to `/explore?near=…&sort=nearest` |

The data they read from the page is listed in [contracts.md](contracts.md#template-and-script-contract). Without JavaScript every page still renders and every link and form works; only the maps, the chart and "Near me" are missing. If MapLibre fails to load, the scripts show a fallback message in the map area.

### Startup tasks

Run once in `web::run` before the server accepts requests. A failure stops startup.

1. Load `.env`, configure logging, create `STORAGE_DIR` and the database folder.
2. Open SQLite (WAL, foreign keys, 5 connections).
3. Apply `migrations/`.
4. `backfill_route_boxes`.
5. `strip_redacted_times`: rewrites redacted GPX files from before times were dropped. Idempotent; unreadable files are logged and skipped.
6. Load and compile `templates/page.hbs`.
7. In `APP_ENV=production`, refuse to start without an HTTPS `BASE_URL`, Brevo settings and a production `MAP_STYLE_URL`.
8. Log a configuration summary (no secrets).

One background job runs for the life of the process: `purge_every_hour` calls `auth::purge_expired` at startup and then hourly, logging counts when it removes anything and an error if it fails.

## External services

| Service | Used for | Called from | Config | If it is unavailable |
| --- | --- | --- | --- | --- |
| **SQLite** | All structured data | Server | `DATABASE_URL` | App does not start; requests fail with 500 |
| **Local disk** | GPX and images | Server | `STORAGE_DIR` | Uploads and file endpoints fail with 500 |
| **Brevo** | Verification email (`POST https://api.brevo.com/v3/smtp/email`) | Server | `BREVO_API_KEY`, `MAIL_FROM` | Register/resend fail with 500 and nothing is saved. Without config (development), the link is logged instead |
| **Map style and tiles** | Base map | Browser | `MAP_STYLE_URL` (default `static/map-style.json`, OSM raster, development only) | Map shows a fallback message; lists still work |
| **unpkg** | MapLibre GL JS 5.24.0 | Browser | Pinned in `templates/page.hbs` | Same as above |
| **Google Fonts** | Manjari and Inter | Browser | `templates/page.hbs` | System fonts are used |
| **Reverse proxy** (deployment) | TLS, request rate limits | In front of the server | Outside this repo | — |
| **Disposable domain list** (GitHub) | Source of `resources/disposable-domains.txt` | `scripts/update_disposable_domains.py`, by hand | — | Existing list keeps working |

Deployment packaging: `Dockerfile` (multi-stage, non-root user) and `compose.yaml` (localhost-only port, one volume for `data/`).
