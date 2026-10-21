# Roadmap

Over There grows in small phases. Each phase has a clear boundary so it ships before the next one starts. Anything outside a phase is listed under "Not planned"; it is not ruled out forever, it just needs a discussion first.

Want to help, or have an idea that is not here? [Open an issue](https://github.com/gnotnek/over-there/issues) so we can talk it through before anyone writes code. Small fixes can go straight to a pull request; see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Phase 0: MVP (done)

GPX upload with segment-safe metrics, public/unlisted/private routes, endpoint hiding, discovery filters, route pages with map and elevation profile, accounts, profiles and moderation. See [architecture.md](architecture.md).

## Phase 1: UI refresh (done)

Make the existing pages look consistent and modern without changing what they do. Design: [specs/2026-10-20-ui-refresh-design.md](specs/2026-10-20-ui-refresh-design.md).

Decided: rebrand to the black, electric lime and white palette with Manjari and Inter, pill buttons and large rounded cards, inspired by the Pilo case study. One radius scale and one control height so rows line up. CSS, the font link and the route page colors only; no new JavaScript or dependencies.

## Phase 2: Routes near me (done)

Opening Explore shows a map and the public routes closest to the rider. Design: [specs/2026-10-20-nearby-routes-design.md](specs/2026-10-20-nearby-routes-design.md).

Boundaries: nearest is the only ranking, 50 km radius, at most 20 results, location is rounded and never stored, distances use only the public (endpoint-hidden) geometry.

## Phase 3: Plan a route on the map (done)

Riders create a route by clicking points on a map; the line follows roads using BRouter, with elevation, and falls back to straight lines. The owner can edit the line of a drawn route later. Climbing is now counted with a moving reference height so gradual climbs count in full, and saved routes were recomputed. Design: [specs/2026-10-21-draw-route-design.md](specs/2026-10-21-draw-route-design.md).

Boundaries: routing runs in the browser, never on the server; uploaded GPX lines stay fixed; at most 100 points; no address search.

## Phase 4: Riders add photos to a route (done)

Signed-in riders add scenery photos to public and unlisted routes others shared, and like them. The route page shows the owner's photos, then the 6 most-liked from other riders; a gallery page holds the rest. Whoever added a photo, or an admin, can delete it; the owner reports other riders' photos instead. The owner can turn contributions off per route. Design: [specs/2026-10-21-community-photos-design.md](specs/2026-10-21-community-photos-design.md).

Boundaries: 5 photos per rider per route, 10 a day for accounts without a verified email; likes rank photos within one route only; no comments, downvotes or notifications.

## Phase 5: Show routes the way Komoot does (to be designed)

- Route cards show the line drawn over a real map instead of the cover photo or plain line drawing, without loading a live map per card (likely a small server-made map image).
- Photos pinned to spots along the route, shown as markers on the route map.

## Not planned

Open an issue if you want to argue for one of these.

- Personalised or "smart" recommendations (ratings, popularity, ride history)
- Turn-by-turn navigation or live tracking
- A saved home location on the account
- Spatial database extensions or moving off SQLite
- Marker clustering, heatmaps, or drawing every route line on the Explore map
- Address or place search (geocoding)
