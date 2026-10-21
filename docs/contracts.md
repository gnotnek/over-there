# Contracts

The promises Over There makes at its boundaries: HTTP endpoints, cookies, file formats, the data the template hands to browser scripts, outbound calls, and configuration. Anything listed here can break a user, a shared link, a browser script or a deployment if it changes silently.

When you change a contract, update this file in the same pull request and say so in the description. Route URLs are shared in the wild, so **never change or remove an existing public URL**; add a new one instead.

Related: [data-catalog.md](data-catalog.md) (what is stored), [service-catalog.md](service-catalog.md) (who implements what).

## HTTP conventions

- **Pages are server-rendered HTML.** There is no JSON API except the route geometry endpoint.
- **Forms** post `application/x-www-form-urlencoded`, except upload and account settings, which use `multipart/form-data`.
- **Success after a POST** is `303 See Other` to a page. Some redirects add `?notice=<message>`, which `static/app.js` shows as a toast.
- **Errors** are a small HTML page with the message as its heading. User-facing messages are written to be shown as-is.

| Status | When |
| --- | --- |
| 400 | Invalid input; the message says what to fix |
| 401 | The page needs a signed-in user, or the email/password is wrong |
| 403 | CSRF check failed, the user may not edit this route, the user is not an admin, or the email is not verified for upload |
| 404 | Unknown page or route, **or a route the viewer may not see**. Private routes return 404 rather than 403, so their existence is not revealed |
| 429 | Sign-in throttle, verification resend cooldown, or daily email quota |
| 500 | Database, storage, template or email-provider failure. Details are logged, not shown |

### Request limits

| Limit | Value |
| --- | --- |
| Request body | 32 MB |
| Multipart text field | 5,000 chars |
| GPX file | 10 MB, ≤ 500,000 XML nodes, 2–100,000 points |
| Image | 5 MB, ≤ 30 megapixels, JPEG/PNG/WebP |

### Access levels used below

- **Anyone**: no sign-in needed.
- **Viewer**: passes `may_view` for the route (public/unlisted: anyone with the URL; private: owner or admin). Otherwise 404.
- **User**: signed in, else 401.
- **Verified**: signed in with a verified email, else 403.
- **Owner**: route author or admin, else 403.
- **Admin**: `users.is_admin = 1`, else 403.
- **CSRF** means the request must pass the [CSRF contract](#csrf).

## Endpoints

| Method | Path | Access | Result |
| --- | --- | --- | --- |
| GET | `/` | Anyone | Home, 4 newest public routes |
| GET | `/explore` | Anyone | Search and map ([details](#get-explore)) |
| GET | `/routes/{slug}` | Viewer | Route page |
| GET | `/routes/{slug}/geometry` | Viewer | Public geometry as GeoJSON ([format](#route-geometry)) |
| GET | `/routes/{slug}/download` | Viewer | GPX file ([details](#get-routesslugdownload)) |
| GET | `/routes/{slug}/cover` | Viewer | Cover image, `image/webp`; 404 if none |
| GET | `/routes/{slug}/images/{id}` | Viewer | Route photo, `image/webp`; 404 if not this route's |
| GET | `/users/{username}` | Anyone | Profile with up to 48 public routes |
| GET | `/users/{username}/avatar` | Anyone | Avatar, `image/webp`; 404 if none |
| GET | `/register`, `/login` | Anyone | Forms |
| POST | `/register` | Anyone, CSRF | Creates account and session → `/dashboard?notice=` |
| POST | `/login` | Anyone, CSRF | Creates session → `/dashboard` |
| POST | `/logout` | Anyone, CSRF | Deletes session → `/` |
| GET | `/verify/{token}` | Anyone | Verifies email → `/dashboard?notice=` |
| GET | `/dashboard` | User | Own routes and totals |
| GET | `/upload` | User | Upload form |
| GET | `/routes/new` | User | Draw-a-route form, same fields as upload with a map instead of the file |
| POST | `/upload` | User, CSRF | Creates route from a GPX or a drawn line → `/routes/{slug}`; 429 past 5 routes a day without a verified email |
| GET | `/routes/{slug}/edit` | Owner | Edit form |
| POST | `/routes/{slug}/edit` | Owner, CSRF | Updates details → `/routes/{slug}` |
| GET | `/routes/{slug}/line` | Owner | Line editor for a drawn route; 404 for uploaded routes |
| POST | `/routes/{slug}/line` | Owner, CSRF | Replaces the line ([details](#post-routesslugline)) → `/routes/{slug}` |
| POST | `/routes/{slug}/delete` | Owner, CSRF | Deletes route and files → `/dashboard` |
| POST | `/routes/{slug}/report` | Anyone, CSRF | Files a report → `/routes/{slug}?notice=`; 404 for private routes |
| GET | `/routes/{slug}/photos` | Same as route | Gallery, 24 per page (`?page=`), most liked first |
| POST | `/routes/{slug}/photos` | User, CSRF | Adds photos ([details](#post-routesslugphotos)) → `/routes/{slug}?notice=` |
| POST | `/routes/{slug}/photos/{id}/like`, `/unlike` | User, CSRF | Likes or unlikes → `back#photo-{id}`; 403 for your own photo |
| POST | `/routes/{slug}/photos/{id}/delete` | Uploader or admin, CSRF | Deletes photo and file → `back?notice=` |
| POST | `/routes/{slug}/photos/{id}/report` | Anyone, CSRF | Reports a photo, same form as route reports → `back?notice=`; 404 for private routes |
| GET | `/account` | User | Settings form |
| POST | `/account` | User, CSRF | Updates bio/avatar → `/account?notice=` |
| POST | `/account/resend-verification` | User, CSRF | New verification email → `/account?notice=` |
| POST | `/account/delete` | User, CSRF | Deletes the account, its routes and files, signs out → `/?notice=` |
| GET | `/admin` | Admin | Stats, latest 100 reports and users |
| POST | `/admin/users/{id}/contributor` | Admin, CSRF | Sets or clears the badge → `/admin` |
| POST | `/admin/reports/{id}` | Admin, CSRF | Dismisses or acts on a report → `/admin` |
| GET | `/static/*` | Anyone | Files from `static/` |

### `GET /explore`

| Query | Type | Rule |
| --- | --- | --- |
| `q` | text | Title contains (case-insensitive) |
| `region` | text | Region contains |
| `cycling_type` | enum | Exact match; empty means any |
| `difficulty` | enum | Exact match; empty means any |
| `min_km`, `max_km` | number | Distance range; negative becomes 0, `max_km` below `min_km` becomes `min_km` |
| `sort` | `newest` (default), `popular`, `nearest` | `popular` = most downloads |
| `page` | integer | 1–10,000, 12 routes per page |
| `near` | `lat,lon` | Rounded to 2 decimals; invalid values are ignored. Never stored or logged |

**Nearby mode** applies when `near` is valid and `sort` is empty or `nearest`: up to 20 public routes within 50 km of the public geometry, nearest first, each with "x km away", no pagination. Other filters still apply.

### `POST /register`

Form: `csrf`, `username` (3–32 of `a-z 0-9 _`, lowercased), `email` (valid, not disposable), `password` (12–128 chars).
Sets `ot_session`. Sends a verification link valid for 48 hours. A taken username or email gives 400 without saying which. If the email provider fails, nothing is saved.

### `POST /login`

Form: `csrf`, `email`, `password`. Wrong credentials give 401 without saying which part was wrong. After 10 failures for one email within 15 minutes, 429 until the window passes.

### `POST /upload`

Multipart fields:

| Field | Required | Rule |
| --- | --- | --- |
| `csrf` | yes | |
| `gpx` | yes, unless drawn | File, see [GPX input](#gpx-input) |
| `line`, `plan` | drawn routes, both or neither | Files, see [Drawn line](#drawn-line). Used instead of `gpx` |
| `title` | yes | 4–100 chars |
| `description` | no | ≤ 2,000 chars |
| `region`, `country` | yes | 1–80 chars |
| `cycling_type` | yes | `road`, `gravel`, `mountain`, `touring`, `bikepacking`, `urban`, `mixed` |
| `difficulty` | yes | `easy`, `moderate`, `hard` |
| `tags` | no | ≤ 160 chars |
| `visibility` | yes | `public`, `unlisted`, `private` |
| `hide_endpoints` | no | `0`, `250`, `500`, `1000` metres; anything else means 0 |
| `cover`, `photo1`–`photo3` | no | Image files |

The same author uploading the same file bytes twice gets 400. A drawn route is checked on the GPX built from its line.

### `POST /routes/{slug}/edit`

Form: `csrf` plus the same text fields as upload, except files and `hide_endpoints`. The slug and endpoint hiding never change after upload.

### `POST /routes/{slug}/line`

Multipart: `csrf`, `line`, `plan` (see [Drawn line](#drawn-line)). Only for routes with a plan. Rebuilds the original and redacted GPX, geometry, elevation, distance, climbing and bounding box, keeping the route's endpoint hiding, details, photos and download count. Duration becomes empty. The duplicate check ignores the route itself.

### `GET /routes/{slug}/download`

- Owner or admin: the original file.
- Everyone else: the redacted file if endpoints are hidden, otherwise the original.
- Headers: `Content-Type: application/gpx+xml`, `Content-Disposition: attachment; filename="{slug}.gpx"`, `Cache-Control: private, no-store`.
- Calls by anyone other than the owner or an admin increment `download_count`.

### `POST /routes/{slug}/report`

Form: `csrf`, `reason` (`misleading`, `copyright`, `unsafe`, `other`), `details` (≤ 1,000 chars). Signed-out visitors may report.

### `POST /routes/{slug}/photos`

Multipart: `csrf`, `website` (honeypot, must be empty), `photo1`–`photo3` (at least one; JPEG, PNG or WebP, ≤ 5 MB each) and `caption1`–`caption3` (≤ 120 chars). 404 when the route is not viewable; 403 when the rider is not the owner and the route is private or `photos_open` is off; 429 when the rider would pass 5 photos on this route or, without a verified email, 10 photos in a day. A refused submission saves nothing and leaves no files.

Photo forms carry `back`, the page to return to. It must be `/routes/{slug}` or a path under it, otherwise the route page is used.

### `POST /account`

Multipart: `csrf`, `bio` (≤ 300 chars), `avatar` (optional image). A new avatar replaces and deletes the old one.

### `POST /account/resend-verification`

Form: `csrf`. Already verified → redirect, no email. 429 if a link was sent in the last 10 minutes or the daily quota is used up.

### `POST /account/delete`

Form: `csrf`, `password` (the account's current password; wrong gives 401 and deletes nothing). On success the account, its routes, their photos, the reports on them and every stored file are removed, `ot_session` is expired, and the response redirects to `/?notice=`. Reports the user filed on other routes stay, without the reporter.

### `POST /admin/users/{id}/contributor`

Form: `csrf`, `verified` (`true` sets the badge, anything else clears it). Has no effect on users whose email is unverified.

### `POST /admin/reports/{id}`

Form: `csrf`, `action`: `dismiss` (status → `dismissed`) or `remove` (route → `private` and status → `removed`, in one transaction). Other actions give 400; an unknown report gives 404.

## Cookies

| Name | Value | Attributes | Purpose |
| --- | --- | --- | --- |
| `ot_session` | 64 hex chars | `Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000` (30 days), `Secure` when `BASE_URL` is HTTPS | Signed-in session. Only its SHA-256 is stored |
| `ot_csrf` | 64 hex chars | `Path=/; SameSite=Lax; Max-Age=2592000`, `Secure` when HTTPS | CSRF token for signed-out visitors. Set on the first page view without one |

Logout expires `ot_session` with `Max-Age=0`.

## CSRF

Every POST form includes `<input type="hidden" name="csrf" value="…">`. The server accepts the request only if:

1. `csrf` is non-empty and equals the expected token: the session's `csrf_token` when signed in, otherwise the `ot_csrf` cookie; and
2. if the request has an `Origin` header, it equals `BASE_URL` exactly.

Otherwise 403. `SameSite=Lax` cookies add a second layer.

## File formats

### GPX input

- UTF-8 XML with a `<gpx>` root element. DTDs and external entities are rejected.
- Points come from `<trk><trkseg><trkpt>` and `<rte><rtept>`, each with `lat` (−90…90) and `lon` (−180…180) attributes.
- Optional per point: `<ele>` (metres) and `<time>` (RFC 3339).
- Each `trkseg` and each `rte` is its own segment. Distance is never measured across a gap between segments.
- Waypoints (`<wpt>`), names and extensions are ignored.

### Drawn line

Sent by `draw.js` as two multipart file parts, because a long line exceeds the text field limit.

- `line`: JSON `[[lon, lat], …]` or `[[lon, lat, ele], …]`, 2–100,000 points, valid coordinates.
- `plan`: JSON `{"points": [[lon, lat], …], "at": [i, …], "straight": [bool, …]}` with 2–100 points. `at` starts at 0, strictly increases and ends at the last line index; `points[k]` equals `line[at[k]]`; `straight[k]` marks the leg from point `k` to `k + 1`.

The server builds a GPX 1.1 file with one `<trk>` named "Planned route" and one `<trkseg>` (`gpx::line_gpx`) and treats it as the uploaded original.

### GPX output (redacted)

Served to visitors when endpoints are hidden: GPX 1.1, one `<trk>` named "Shared route", one `<trkseg>` per public segment, keeping each kept point's `lat`, `lon` and `<ele>`. `<time>` is dropped, so the file does not reveal when the owner rode. Points within the hiding radius of the start or finish are removed. Nothing else from the original file is carried over.

### Route geometry

`GET /routes/{slug}/geometry` returns `Content-Type: application/geo+json` and `Cache-Control: private, max-age=60`, with a bare GeoJSON geometry (not a Feature):

```json
{"type": "MultiLineString", "coordinates": [[[121.0, 25.0], [121.01, 25.01]]]}
```

Coordinates are `[lon, lat]`, endpoint-hidden and simplified (~8 m tolerance). One line per segment.

### Elevation profile

Embedded in the route page (not an endpoint): a JSON array of `[distance_m, elevation_m, lon, lat]`, distance measured along the public geometry.

## Template and script contract

`templates/page.hbs` passes data to the browser scripts through element IDs and `data-*` attributes. Renaming one breaks the script silently, so change both sides together.

| Element | Attribute | Value | Read by |
| --- | --- | --- | --- |
| `#route-map` | `data-slug` | Route slug, used to fetch geometry | `route.js` |
| `#route-map`, `#explore-map` | `data-style` | `MAP_STYLE_URL` | `route.js`, `explore.js` |
| `#elevation-chart` | `data-points` | [Elevation profile](#elevation-profile) JSON | `route.js` |
| `#explore-map` | `data-pins` | JSON `[{"slug","title","lon","lat"}]`, first point of each listed route's public geometry | `explore.js` |
| `#explore-map` | `data-near` | `"lat,lon"` in nearby mode, else empty | `explore.js` |
| `.route-card` | `data-slug` | Links a card to its pin | `explore.js` |
| `#near-me`, `#near-status` | — | Location button and its status text | `explore.js` |
| `#draw-map` | `data-style`, `data-routing` | `MAP_STYLE_URL`, `ROUTING_URL` | `draw.js` |
| `#draw-map` | `data-line`, `data-plan`, `data-cycling-type` | Line editor only: the saved line and plan ([Drawn line](#drawn-line)) and the route's cycling type | `draw.js` |
| `#draw-undo`, `#draw-clear`, `#draw-locate`, `#draw-follow`, `#draw-stats`, `#draw-status`, `#draw-credit`, `#draw-submit` | — | Planner controls and text | `draw.js` |
| `input[name=line]`, `input[name=plan]` | — | Hidden file inputs filled on submit | `draw.js` |
| `.map-fallback` | — | Placeholder replaced with an error message | all map scripts |
| `?notice=` | query | Message shown as a toast | `app.js` |

`explore.js` sends location only as `near=<lat 2dp>,<lon 2dp>&sort=nearest`. `draw.js` never sends the rider's location; it sends placed points to `ROUTING_URL` as `?lonlats=<lon,lat>|<lon,lat>&profile=<fastbike|gravel|mtb|trekking>&alternativeidx=0&format=geojson`, one request per leg, 10 s timeout.

## Outbound: Brevo

`POST https://api.brevo.com/v3/smtp/email`, header `api-key: $BREVO_API_KEY`, JSON body:

```json
{"sender": {"name": "Over There", "email": "$MAIL_FROM"},
 "to": [{"email": "<user>"}],
 "subject": "Verify your Over There email",
 "textContent": "Verify your email: <BASE_URL>/verify/<token>\nThis link expires in 48 hours."}
```

Any non-2xx response is treated as a failure. At most 250 sends per UTC day.

## Configuration

Read from the environment (and `.env`) at startup. See [.env.example](../.env.example).

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | `sqlite://data/over-there.db` | SQLite file; created if missing |
| `STORAGE_DIR` | `data/files` | Private file storage; created if missing |
| `BASE_URL` | `http://127.0.0.1:3000` | Public origin. Used for email links, canonical URLs, the CSRF `Origin` check, and the `Secure` cookie flag when it is HTTPS |
| `LISTEN_ADDR` | `127.0.0.1:3000` | Bind address (`0.0.0.0:3000` in Docker) |
| `MAP_STYLE_URL` | `/static/map-style.json` | MapLibre style for every map |
| `ROUTING_URL` | `https://brouter.de/brouter` | BRouter endpoint the browser calls while drawing a route; point it at a self-hosted BRouter to stop sending points to brouter.de |
| `BREVO_API_KEY`, `MAIL_FROM` | unset | Email delivery. If unset, verification links are logged |
| `APP_ENV` | unset | `production` refuses to start without an HTTPS `BASE_URL`, both Brevo variables and a non-default `MAP_STYLE_URL` |
| `RUST_LOG` | `over_there=info,tower_http=info` | Log filter |

## Logs

Logs go to stdout through `tracing`, filtered by `RUST_LOG`. Operators, dashboards and alerts depend on these messages and field names, so treat them like any other contract.

**Privacy rule:** logs may contain user IDs, route IDs and report IDs. They never contain emails, passwords, tokens, cookie values, query strings (which can carry the `near` location) or file contents. The one exception is the development-only verification link, logged when no email provider is configured.

### Every request

Each request runs in a `request` span with `id` (a random UUID for correlating lines), `method` and `path` (no query string). When it finishes, `tower_http` logs `finished processing request` at INFO with `status` and `latency` in milliseconds. Every line logged while handling the request carries the span's fields.

| Level | Message | Fields | When |
| --- | --- | --- | --- |
| INFO | `request rejected` | `status`, `reason` (the user-facing message) | Any 4xx `AppError` |
| ERROR | `internal error` | `error` | Any 5xx `AppError` |
| WARN | `csrf check failed` | — | A POST failed the [CSRF contract](#csrf) |

### Events

| Level | Message | Fields |
| --- | --- | --- |
| INFO | `account registered` | `user_id` |
| INFO | `email verified` | `user_id` |
| INFO | `verification email sent` | `provider` |
| INFO | `verification link resent` | `user_id` |
| WARN | `Development email verification link` | `email`, `link` (development only) |
| WARN | `daily verification email quota reached` | — |
| INFO | `signed in` | `user_id` |
| WARN | `sign-in failed` | `known_account` (whether the email exists) |
| WARN | `sign-in throttled` | — |
| INFO | `signed out` | `user_id` |
| INFO | `account updated` | `user_id`, `avatar_changed` |
| INFO | `account deleted` | `user_id`, `routes` |
| INFO | `route uploaded` | `route_id`, `user_id`, `visibility`, `points`, `segments`, `distance_m`, `hide_endpoints_m`, `photos`, `drawn` |
| INFO | `route line updated` | `route_id`, `user_id`, `points`, `distance_m` |
| INFO | `route updated` | `route_id`, `user_id`, `visibility` |
| INFO | `route deleted` | `route_id`, `user_id` |
| INFO | `route downloaded` | `route_id`, `original` (served the original file), `counted` |
| INFO | `route reported` | `report_id`, `route_id`, `reason`, `signed_in` |
| INFO | `contributor badge changed` | `admin_id`, `user_id`, `verified` |
| INFO | `report resolved` | `admin_id`, `report_id`, `route_id`, `action` |
| WARN | `stored file not removed` | `key`, `error` |

### Startup and background

| Level | Message | Fields |
| --- | --- | --- |
| INFO | `configuration loaded` | `env`, `base_url`, `storage`, `email` (`brevo` or `log`), `custom_map_style` |
| INFO | `Over There listening` | `addr` |
| INFO | `route boxes backfilled` | `updated` |
| INFO | `times removed from redacted GPX files` | `stripped` |
| WARN | `redacted GPX unreadable` / `unparsable` / `not rewritten` | `key`, `error` |
| INFO | `expired records purged` | `sessions`, `verifications`, `attempts` |
| ERROR | `purging expired records failed` | `error` |
