# Phase 3: Plan a route on the map

Status: done. Part of the [roadmap](../roadmap.md).

## Why

Today a route can only be shared by uploading a GPX file. A rider who wants to plan a ride they have not done yet, or who simply has no file, cannot use Over There at all. The moment this serves: a rider opens the map, looks around an area, and puts together a ride by clicking where they want to go, with the line following real roads.

User story: I want to plan a ride. I open the map, click a start, click where I want to pass through, and the line follows the roads. I see the distance and climbing grow. I add the usual details and publish. Later I can come back and move the line.

## Goals

- A signed-in rider can create a route by clicking points on a map, without a GPX file.
- Between clicks the line follows roads and paths suited to the route's cycling type.
- A drawn route has elevation, so its page has an elevation chart and ascent like an uploaded one.
- A drawn route becomes an ordinary route: same detail page, map, elevation chart, GPX download, visibility, endpoint hiding, editing and deletion.
- The owner can reopen a drawn route and change its line.

## Non-goals

- Changing the line of an **uploaded** GPX route. Its line stays fixed as today. (Converting an upload into an editable plan could come later.)
- Turn-by-turn directions, cue sheets, surface or road-type breakdowns, or a choice between alternative routes.
- Address or place search (still under "Not planned"). The rider finds their area by panning or with "Center on me".
- Map-style route cards and photos placed along the route: that is Phase 4.
- Hosting our own routing server. The design keeps it possible through configuration.

## Anti-goals (a failure even if it works)

- A drawn route skips any check an uploaded route goes through: GPX validation, point limits, duplicate check, endpoint hiding, visibility, and any upload guards added to the `upload` handler (such as the honeypot and daily limit on `feat/optional-email-verification`).
- Endpoint hiding leaks the start or finish of a drawn route, for example through the stored plan being served to visitors.
- The planner stops working entirely when the routing service is down.
- A new JavaScript library or build step is added.
- The server starts calling the routing service. Routing stays in the browser, like map tiles.

## Decisions made with the user

- **Lines follow roads**, using [BRouter](https://brouter.de/). It is free, open source and built for cycling, it can be self-hosted, and each returned point carries elevation. That also answers where elevation comes from: BRouter's points have `[lon, lat, ele]`.
- **Straight-line fallback** when the routing service fails, so the rider can still save.
- **The owner can edit the line of a drawn route** after saving. Uploaded routes keep a fixed line.
- **This is Phase 3.** Map-style previews and photos along the route become Phase 4.

Checked on 21 October 2026: `https://brouter.de/brouter?lonlats=<lon,lat>|<lon,lat>&profile=<p>&alternativeidx=0&format=geojson` answers browser requests (`Access-Control-Allow-Origin: *`) with a GeoJSON `LineString` of `[lon, lat, ele]` and the properties `track-length` and `filtered ascend`. The profiles `fastbike`, `gravel`, `mtb` and `trekking` exist on the public server.

## Behaviour

### Creating

1. **Entry.** The Share page (`/upload`) keeps the GPX form and gains a line: "No GPX file? Draw it on the map", linking to `/routes/new`. The dashboard's "Add route" stays on `/upload`.
2. **Page.** `/routes/new` (signed in, same access as `/upload`) shows the same route form as the Share page, with the GPX file section replaced by a map. The details, photos, visibility and endpoint-hiding fields are the same fields, rendered from the same template markup, not a copy.
3. **Drawing.** Clicking the map adds a point at the end of the line. A new point is joined to the previous one by asking BRouter for that leg only. Points are markers that can be dragged; dragging re-routes only the two legs that touch it. Clicking a marker removes it and re-routes the gap. "Undo" removes the last action's point, and "Clear" starts over. At most 100 points.
4. **Profile.** The routing profile follows the form's cycling type when a leg is routed: road → `fastbike`, gravel → `gravel`, mountain → `mtb`, everything else → `trekking`. Changing the cycling type affects legs routed afterwards and does not re-route existing ones.
5. **Follow roads.** A "Follow roads" switch, on by default. When off, new legs are straight lines. This covers ferries, trails missing from OpenStreetMap, and squares.
6. **Fallback.** If a BRouter request fails or times out after 10 seconds, that leg is drawn straight and dashed, and a status line says "Couldn't follow roads here, so this part is a straight line." A straight leg adds no points of its own: its two ends keep any elevation they got from the routed legs next to them, so the climb across it counts as the difference between its ends. A route drawn entirely with straight legs has no elevation, like an uploaded GPX without `<ele>`. Adding an elevation data source is out of scope.
7. **Live numbers.** Below the map: distance and ascent of the line so far, computed in the browser with the same rules as the server (haversine distance, and the [climbing rule](#climbing-rule-fix-all-routes)), so they match the saved route.
8. **Center on me.** A button asks the browser for its location and centers the map. The location stays in the browser. Without it, the map opens on a world view.
9. **Credit and privacy.** A line under the map: "Routing by BRouter. The points you place are sent to brouter.de." (The host shown follows the configured URL.)
10. **Publish.** The button is disabled until the line has two points. On submit, the browser sends the form as multipart to `POST /upload` with two extra file parts instead of `gpx`:
    - `line`: JSON array of `[lon, lat]` or `[lon, lat, ele]` for the whole line, in order.
    - `plan`: JSON `{"points": [[lon, lat], ...], "at": [i, ...], "straight": [bool, ...]}`, where `points` are the snapped point positions, `at[k]` is the index in `line` where point `k` lies, and `straight[k]` says whether the leg from point `k` to point `k+1` is straight.
    They are file parts because the multipart text limit is 5,000 characters and a long line is far larger.
11. **Server.** `upload` accepts either a `gpx` file or a `line` + `plan` pair (both present or 400). For a line, it validates the plan against the line, builds a GPX with `gpx::line_gpx`, and from there runs the existing path unchanged: parse, SHA-256 duplicate check, endpoint hiding, redacted GPX, geometry, box, elevation, insert. The plan is stored in a new column. The generated GPX is stored as the route's original GPX.
12. **Errors.** Validation errors come back as the usual 400 error page, as with uploads. Failing to keep the drawing on a 400 is accepted for this phase (see "Left to the builder").

### Editing the line

13. **Entry.** The route edit page shows "Edit the line" (linking to `/routes/{slug}/line`) only when the route has a plan. For uploaded routes it says nothing new.
14. **Page.** `/routes/{slug}/line` (owner only, 403 otherwise; 404 when the route has no plan) shows the same map editor without the details form, preloaded with the saved line and points. It does not call BRouter until the rider changes something.
15. **Save.** `POST /routes/{slug}/line` with `csrf`, `line` and `plan` (multipart). The server validates and builds the GPX as on create, then recomputes and replaces: original GPX file, redacted GPX file (when endpoints are hidden), `gpx_sha256`, geometry, elevation profile, distance, gain, loss, duration (always empty for a drawn route), bounding box, plan, `updated_at`. Slug, title, details, visibility, endpoint hiding, photos and download count stay. Old files are deleted after the database update commits; on a failed update the new files are deleted instead. The duplicate check ignores the route itself.
16. **Redirect** to the route page.

## Climbing rule fix (all routes)

Decided with the user after measuring: `gpx::parse` ignores any rise or drop of 3 m or less **between neighbouring points**. Dense lines, from BRouter or from a GPS recording a point every second, climb in small steps, so most of a real climb is thrown away. On a 5.7 km BRouter line near Innsbruck, BRouter reports 129 m of climbing, the current rule gives 54 m, and the corrected rule gives 133 m.

- **New rule**, for gain and loss alike: keep a reference height, starting at the first point with elevation. When the height rises more than 3 m above the reference, add the difference to gain and move the reference to the new height. When it drops more than 3 m below, add the difference to loss and move the reference. Smaller wobbles leave the reference alone. Segments are still not joined: each segment starts its own reference.
- It applies to every route: uploads, drawn routes and line edits. The browser's live numbers use the same rule.
- **Existing routes are recomputed** from their original GPX (`gpx_key`) by a startup task, like `backfill_route_boxes`. It must run once, not re-parse every file on every start. The mechanism is the builder's choice; one option is a migration that adds a marker. A file that is missing or fails to parse is logged and skipped, and its stored numbers are left as they are.
- Tests: many 1 m steps up a hill add up to the full climb; ±2 m noise on flat ground counts as no climbing; the existing tests still pass.

## Data

One migration adds `routes.plan_json TEXT NULL`. Null means an uploaded route. It is **owner-only data**: it holds the exact start and finish even when endpoints are hidden, so it is never sent to anyone but the owner (the line editor page) and never appears in public JSON, geometry or downloads.

## Domain

- `gpx::line_gpx(line: &[Point]) -> Vec<u8>`: one `<trk>` with one `<trkseg>`, `<ele>` when present, no `<time>`. It follows `redacted_gpx`. Test: a synthetic line with and without elevation round-trips through `gpx::parse` with the same points, distance and elevation.
- A plan check (in `gpx.rs` beside it, or `route.rs`): the line has 2–100,000 finite, in-range points; the plan has 2–100 points; `at` starts at 0, strictly increases and ends at the last line index; `straight` has one entry per leg; each plan point equals `line[at[k]]`. Test: a valid plan passes, and each broken rule is rejected.

## Browser

One new script, `static/draw.js`, loaded only on `/routes/new` and `/routes/{slug}/line`, with the same MapLibre version and map style as the other pages and the palette's line style (lime casing, black line; dashed for straight legs). It reads its configuration from `data-*` attributes on a `#draw-map` element: map style, routing URL, and on the line editor, the saved line and plan. The routing URL comes from a new optional setting `ROUTING_URL` (default `https://brouter.de/brouter`), so a self-hosted BRouter is a configuration change.

## Docs to update

- `architecture.md`: replace "no … route planner" with what Phase 3 allows; add BRouter to the context diagram.
- `roadmap.md`: Phase 3 (this) and Phase 4 (map-style route cards and photos placed along the route, to be designed).
- `contracts.md`: `/routes/new`, the `line`/`plan` parts of `POST /upload`, `GET`/`POST /routes/{slug}/line`, the `#draw-map` attributes, `ROUTING_URL`.
- `data-catalog.md`: `plan_json` and its class; `gpx_key` for drawn routes is the generated GPX; the new climbing rule for `elevation_gain_m` and `elevation_loss_m`.
- `service-catalog.md`: `line_gpx`, the plan check, `draw.js`, BRouter as an external service (browser-called; if down, legs fall back to straight lines).
- `.env.example`: `ROUTING_URL`.

## Tests

- Domain: `line_gpx` round-trip; plan check accepts a valid plan and rejects each broken rule.
- Web, all with synthetic coordinates:
  - posting `line` + `plan` to `/upload` creates a public route whose detail page renders with the title, distance and an elevation chart;
  - `line` without `plan`, or neither `gpx` nor `line`, gives 400;
  - the owner saves a new line through `/routes/{slug}/line` and the distance changes, the slug stays, and the old GPX file is gone;
  - another user gets 403 on `/routes/{slug}/line`, and an uploaded route gets 404 there;
  - the plan never appears in the route page or geometry served to a visitor of a route with hidden endpoints.

## Left to the builder

- How `draw.js` submits the files (setting a hidden file input with `DataTransfer` before a normal submit, or `fetch` with `FormData` then following the redirect).
- Whether a 400 on create keeps the drawing (for example by keeping the line in `sessionStorage`). Nice to have, not required.
- Exact markup and CSS, following the Phase 1 style; how the switch, Undo, Clear and status line look.
- Whether `/routes/new` posts to `/upload` or gets its own `POST /routes/new` that calls the same code; either way, one code path.
## Calls I made (not discussed with the user)

- Build on `main`. `feat/optional-email-verification` (honeypot, daily limit for unverified accounts) also rewrites `upload` and the Share form; whichever branch merges second resolves the conflict. Because a drawn route is created through `upload`, it gets those guards on merge. `/routes/new` uses the same gate as the Share page. Saving a line is an update to a route the owner already has, so it does not count toward the daily limit and needs no honeypot.
- URL names: `/routes/new` and `/routes/{slug}/line`.
- 100 points maximum, 10 second routing timeout.
- Profile follows cycling type instead of a separate control, and is not stored.
- Line edits keep the route's endpoint hiding; changing it still needs a new route.
- A "Follow roads" switch, in addition to the automatic fallback.
- `ROUTING_URL` setting with brouter.de as the default.
- Drawn routes have no duration.
