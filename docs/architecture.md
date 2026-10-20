# Over There — architecture and MVP decisions

## Product reading

Over There is a public library of GPX routes, starting with cycling. A visitor finds a ride by place and effort, studies the line and elevation, then downloads the original file. A contributor uploads a GPX, checks derived facts, chooses visibility, and maintains a public profile. There is no feed, following system, route planner, or navigation.

The [Komoot home page](https://www.komoot.com/) demonstrates how activity and destination browsing, map context, route difficulty, and clear route presentation aid discovery. Its planning, navigation, and social features are outside this MVP. The supplied [CariRute URL](https://gpx.ai.id/) was unavailable to the research tool on 20 October 2026, so no specific claim about its current features informs this design.

## Stack decision

| Option | Rust learning and upkeep | SSR/SEO | Maps | Deployment | Decision |
| --- | --- | --- | --- | --- | --- |
| Leptos full stack SSR | Shared Rust types and reactive UI; more macros, hydration, WASM and feature gates to learn | Strong | JS interop needed for MapLibre | Rust server plus WASM bundle | Good future option |
| Leptos + Axum | Flexible HTTP endpoints with Leptos UI; largest integration surface | Strong | JS interop needed | Same two build targets | Useful when client reactivity grows |
| Axum API + separate frontend | Familiar separation, more duplication and client state | Requires another SSR framework | Straightforward | Two applications | Too much for a solo MVP |
| **Axum + Handlebars SSR + small JS islands** | **Direct Rust request/data flow, ordinary templates, smallest build** | **Strong** | **MapLibre on detail pages** | **One binary plus assets** | **Selected** |

Axum and its multipart support are documented in the [official crate docs](https://docs.rs/axum/latest/axum/). [Leptos SSR deployment](https://book.leptos.dev/deployment/ssr.html) entails a server build and a client WASM build. MapLibre is a browser WebGL library with [GeoJSON sources, map bounds, navigation and fullscreen controls](https://maplibre.org/maplibre-gl-js/docs/). Keeping the map in JavaScript is a practical boundary, not a loss of application logic to JavaScript.

SQLite is selected for a single affordable server. WAL mode and indexed, paginated queries are enough for an initial route library. PostgreSQL becomes appropriate for multiple writers, replicas, geographic search, or significant moderation traffic. SQLx gives async queries and migrations; this code uses runtime bound queries because they work without a build-time database. The original GPX and processed images live under a private storage root and are served through authorization checks. A storage trait can be introduced when object storage is actually needed.

GPX is parsed with `roxmltree`, DTD disabled and a node limit, so the service has explicit control over track segment boundaries and missing fields. The [parser's documented defaults](https://docs.rs/roxmltree/latest/roxmltree/struct.ParsingOptions.html) reject DTDs. Passwords use `ring` PBKDF2-HMAC-SHA256 with per-user random salt; session tokens are random and only their SHA-256 digest is stored. Image uploads are decoded and re-encoded to WebP to discard metadata and avoid serving untrusted source formats.

MapLibre renders simplified GeoJSON. The original remains intact for downloads. The default OSM raster style is for development and modest testing; public deployment must configure a suitable tile service and respect the [OSM tile usage policy](https://operations.osmfoundation.org/policies/tiles/). Route previews use generated vector thumbnails, so discovery does not load dozens of live maps.

## Domain boundaries

The code uses a pragmatic domain driven design. `src/domain/route.rs` owns route validation, visibility and access rules; `src/domain/gpx.rs` owns GPX geometry and derived facts. `src/identity/auth.rs` owns password and session mechanics; `src/identity/email.rs` owns email validation and the local disposable-domain check. `src/infrastructure/routes.rs` maps the persisted route record and queries it with SQLx. `src/presentation/web.rs` is the HTTP adapter: it reads forms, calls domain rules, coordinates persistence and storage, and renders templates. The database schema stays explicit and serves as the boundary for future extraction of more repository methods. New activity types should enter through the route domain, and new storage services should replace the file adapter without changing GPX computation or access rules.

This is deliberately a small domain model rather than a forest of traits and generic repositories. The tests focus on domain invariants: private access, segment boundaries, incomplete metrics, DTD rejection and endpoint hiding.

## Data and visibility

The schema has users, sessions, routes, route_images, route_reports and email_verifications. A route owns its GPX file and public geometry. Category (`cycling`) is separate from cycling type so hiking and running can be added without changing route ownership. `visibility` is one of public, unlisted and private. Public discovery queries only `public`; unlisted detail and GPX require possession of the unguessable slug; private detail and downloads require owner or administrator. Map geometry and image endpoints follow the same gate. Email verification and contributor verification are distinct fields; only an administrator sets the contributor flag.

The uploader controls endpoint hiding before public sharing. When enabled, a published geometry copy omits a user-selected radius from each end; the original GPX stays private to the owner and the public download is a generated redacted GPX. The interface must make this distinction explicit. The initial implementation should never expose raw file paths or GPS data through predictable static URLs.

## Primary flows and page structure

1. Visitor: home → explore with title/region/type/difficulty/distance/sort → route detail/map/profile → GPX download.
2. Contributor: register → verify email → upload GPX → review computed metrics and privacy → publish → dashboard/edit.
3. Moderator: admin report queue → inspect route → remove content or dismiss → manage contributor badge.

Navigation has Explore, Share a route, Dashboard (when signed in), and account access. The landing page uses the route cue sheet as its visual object; the route trace is the distinctive graphic. The working screens use a quiet green tinted frame around a white map and content sheet. Reusable elements are the navigation shell, route preview, metric row, state label, form field, map, and elevation profile. On phones, the explore filters become a stacked form and the map precedes route details.

## Delivery order and deployment

1. Core: schema, authentication, GPX parser, storage and ownership checks.
2. Routes: upload/edit/delete, public discovery, detail, geometry and download.
3. Identity: profiles, account settings, verification and image handling.
4. Trust: reports, moderation and contributor badges.
5. Operate: tests, Docker, backups, logging, rate limits and production configuration.

For a single host, back up the SQLite database with its online backup mechanism and the entire private storage directory as one coordinated snapshot. Restore both together. Put TLS and request rate limiting at the reverse proxy and set a production map style and email provider. The app also enforces upload size and authorization internally. See the README for executable setup.

## Rust concepts in this implementation

Handlers borrow shared state through Axum's `State<Arc<AppState>>`; the pool and template registry are shared safely without copying their internals. GPX parsing borrows the XML text while walking nodes, then creates owned route data before the document is dropped. `Result` and `?` propagate parse, database, and filesystem errors; user input errors are turned into 4xx responses at the HTTP boundary. Structs for parsed geometry and route records keep domain invariants explicit without a large repository or trait hierarchy.
