# Phase 2: Routes near me

Status: design agreed, not started. Part of the [roadmap](../roadmap.md).

## Why

Explore is a filter form and a list of route cards. That works when you already know what you want. The common moment is different: a rider opens the app on a free morning and wants to know what they can ride from where they are. The first thing they see should answer that.

User story: I want to find a new route. I open Explore, I see a map, and I see recommendations for the routes nearest to me.

## Goals

- Explore opens with a map and a short list of the nearest public routes.
- Works for signed-out visitors as well as members.
- Respects every existing privacy rule, especially endpoint hiding.

## Non-goals

See "Not planned" in the roadmap: no smart ranking, navigation, saved home location, spatial extensions, clustering or geocoding.

## Anti-goals (a failure even if it works)

- Sorting by distance reveals roughly where a contributor starts their rides. Distances must never be computed from the original GPX, only from the stored public geometry (`routes.geometry_json`, already trimmed by `hide_endpoints_m` and simplified).
- The rider's location is stored, logged in full precision, or tied to their account.
- Explore stops working for people who decline location access.
- A new JavaScript library, or a second map library, gets added.

## Behaviour

1. **Location.** Explore has a "Show routes near me" button over the map. If the browser reports location permission is already granted (Permissions API), the page uses it straight away without the button. If permission is denied, unavailable or times out, Explore stays as it is today (newest first) and says why in one short line.
2. **Precision.** The browser rounds latitude and longitude to two decimals (about 1 km) before sending them. The server never stores them and the request log must not record the query string for this route. Note that the current `TraceLayer::new_for_http()` span includes the full URI, so it needs a custom span that logs the path only.
3. **Request.** Server-rendered, like the rest of the site: the page reloads `/explore` with `near=<lat>,<lon>` added to the existing filter query. The server validates the pair (finite, lat −90..90, lon −180..180); an invalid pair is ignored as if absent.
4. **Results.** With `near` present: public routes only, existing filters still apply (name, region, type, difficulty, min/max km), distance ≤ 50 km, nearest first, at most 20 results, no pagination. The sort control shows "Nearest" and switching to another sort drops `near`.
5. **Distance.** Great-circle (haversine) distance from the rider to the closest vertex of the route's public geometry. Each card shows it, e.g. "3.2 km away".
6. **Empty.** No public routes within 50 km: say so, and link to the unfiltered newest list.
7. **Map.** The existing MapLibre setup and map style, now also loaded on Explore. One pin per listed route, placed at the first point of its public geometry (a point already visible on the route's own page). The rider's rounded position gets a distinct marker. The map fits all pins. Clicking a pin highlights its card and the reverse; clicking a card opens the route page as today. Without `near`, the map shows pins for the routes on the current page.
8. **Layout.** Desktop: map and list side by side. Phone: map first, list below, matching the existing rule that the map precedes details on phones.

## Data

Add four nullable columns to `routes`: the bounding box of the public geometry (`min_lat`, `max_lat`, `min_lon`, `max_lon`) with an index. They are written wherever `geometry_json` is written (upload and edit) by one domain function in `src/domain/gpx.rs`. A migration adds the columns; existing rows are backfilled once.

Query: select public routes whose box intersects the rider's 50 km box (with filters), then compute exact distances in Rust, drop anything over 50 km, sort and keep 20. At the expected scale this needs no spatial index.

## Tests

- Domain: bounding box and nearest-vertex distance from a synthetic MultiLineString; a route whose hidden start is near the rider but whose public line is not must not count as near.
- Web: `near` with filters returns only public routes within 50 km, nearest first, capped at 20; invalid `near` falls back to the normal list; unlisted and private routes never appear.

## Left to the builder

- Backfill mechanism (SQLite JSON functions in the migration, or a one-off pass at startup).
- Exact markup and CSS, following the Phase 1 style.
- How pin–card highlighting is wired, within the existing small JavaScript files.
- Whether the bounding-box helper lives in `gpx.rs` or a small new domain module.
