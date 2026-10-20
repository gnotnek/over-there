# Roadmap

Over There grows in small phases. Each phase has a clear boundary so it ships before the next one starts. Anything outside a phase is listed under "Not planned"; it is not ruled out forever, it just needs a discussion first.

Want to help, or have an idea that is not here? [Open an issue](https://github.com/gnotnek/over-there/issues) so we can talk it through before anyone writes code. Small fixes can go straight to a pull request; see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Phase 0: MVP (done)

GPX upload with segment-safe metrics, public/unlisted/private routes, endpoint hiding, discovery filters, route pages with map and elevation profile, accounts, profiles and moderation. See [architecture.md](architecture.md).

## Phase 1: UI refresh (done)

Make the existing pages look consistent and modern without changing what they do. Design: [specs/2026-10-20-ui-refresh-design.md](specs/2026-10-20-ui-refresh-design.md).

Decided: rebrand to the black, electric lime and white palette with Manjari and Inter, pill buttons and large rounded cards, inspired by the Pilo case study. One radius scale and one control height so rows line up. CSS, the font link and the route page colors only; no new JavaScript or dependencies.

## Phase 2: Routes near me (next)

Opening Explore shows a map and the public routes closest to the rider. Design: [specs/2026-10-20-nearby-routes-design.md](specs/2026-10-20-nearby-routes-design.md).

Boundaries: nearest is the only ranking, 50 km radius, at most 20 results, location is rounded and never stored, distances use only the public (endpoint-hidden) geometry.

## Not planned

Open an issue if you want to argue for one of these.

- Personalised or "smart" recommendations (ratings, popularity, ride history)
- Turn-by-turn navigation or live tracking
- A saved home location on the account
- Spatial database extensions or moving off SQLite
- Marker clustering, heatmaps, or drawing every route line on the Explore map
- Address or place search (geocoding)
