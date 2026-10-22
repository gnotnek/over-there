# Riders Add Photos to a Route Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Signed-in riders add scenery photos to routes others shared, like them, delete their own and report others', with the owner able to turn contributions off.

**Architecture:** `route_images` gains an uploader, caption and timestamp; a `photo_likes` table and a nullable `route_reports.image_id` carry likes and photo reports; `routes.photos_open` is the owner's switch. Access rules live in `domain::route`; handlers in `presentation::web` follow the existing form-post-redirect pattern, with limits enforced inside the insert statement like the daily route limit. No new JavaScript.

**Tech Stack:** Rust (Axum, SQLx/SQLite), Handlebars, CSS.

**Spec:** [docs/specs/2026-10-21-community-photos-design.md](../specs/2026-10-21-community-photos-design.md)

## Global Constraints

- 1–3 photos per submission, fields `photo1..photo3` with `caption1..caption3`; caption ≤ 120 characters, trimmed; JPEG/PNG/WebP through `optimized_image(_, 1600)`.
- `PHOTOS_PER_RIDER_PER_ROUTE = 5` (owner's upload photos count); `UNVERIFIED_DAILY_PHOTOS = 10` per rolling day across all routes. Both enforced in the `INSERT … SELECT … WHERE` that saves each photo.
- Route page strip: owner's photos (position order), then 6 others by likes desc, `created_at` desc, `id`. Gallery: all photos, same order, 24 per page.
- Alt text: caption, else `Photo of {title} by {username}`.
- Owner cannot delete other riders' photos; uploader or admin can. A rider cannot like their own photo.
- Every photo endpoint 404s when `can_view` fails or the photo does not belong to the slug's route.
- Honeypot `website` and CSRF on every new form. Redirect targets come from a `back` field and must start with `/routes/{slug}`; anything else falls back to `/routes/{slug}`.
- `cargo fmt --check`, `cargo check`, `cargo test` pass after every task. Synthetic data only.

## Review Focus

- A like, unlike, delete or report on a photo id from a different route: 404, nothing changed.
- `back=//evil.example` or `back=https://evil.example`: redirect lands on the route page instead.
- A rider with 4 photos on a route submits 3: the whole submission is refused, no rows and no files left.
- A submission with no file chosen in any of the three fields: 400 "Choose at least one photo".
- Liking a photo on a private route as another rider: 404.

Lines 1, 2 and 5 get tests in Task 4; lines 3 and 4 in Task 2.

---

### Task 1: Schema, rules and the owner switch

**Files:**
- Create: `migrations/20261022000002_community_photos.sql`
- Modify: `src/domain/route.rs` (`RouteDraft`, new rule fns, tests)
- Modify: `src/infrastructure/routes.rs` (`RouteRecord.photos_open: i64`)
- Modify: `src/presentation/web.rs` (`upload` and `edit_route` save `photos_open`; upload's photos set `uploader_id`, `created_at`; `route_json` exposes `photos_open`)
- Modify: `templates/page.hbs` (checkbox on upload/draw and edit forms)

**Interfaces:**
- Produces: `route::may_add_photo(visibility: Visibility, photos_open: bool, owner_id: &str, actor_id: &str) -> bool`; `route::may_like_photo(can_view: bool, uploader_id: &str, actor_id: &str) -> bool`; deleting uses existing `may_edit(uploader_id, actor_id, is_admin)`. `RouteDraft.photos_open: bool` (true when the `photos_open` field is present). Columns as below.

- [ ] **Step 1: Write the failing tests**
  - `domain::route::tests::photo_rules_follow_visibility_switch_and_authorship`: owner may add to own private route with switch off; other rider may add to public and unlisted only when switch on; nobody else may add to private; `may_like_photo(true, "a", "a")` is false, `(true, "a", "b")` true, `(false, "a", "b")` false.
  - `web::tests::owner_switch_is_saved_on_upload_and_edit`: upload with `photos_open=1` stores 1; edit without the field stores 0.
- [ ] **Step 2: Run `cargo test photo_rules owner_switch`; expect compile failure.**
- [ ] **Step 3: Migration**

```sql
ALTER TABLE routes ADD COLUMN photos_open INTEGER NOT NULL DEFAULT 1;
ALTER TABLE route_images ADD COLUMN uploader_id TEXT REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE route_images ADD COLUMN caption TEXT NOT NULL DEFAULT '';
ALTER TABLE route_images ADD COLUMN created_at TEXT;
UPDATE route_images SET uploader_id=(SELECT author_id FROM routes WHERE routes.id=route_images.route_id),
  created_at=(SELECT created_at FROM routes WHERE routes.id=route_images.route_id);
CREATE INDEX route_images_uploader_idx ON route_images(uploader_id, created_at);
CREATE TABLE photo_likes (
 image_id TEXT NOT NULL REFERENCES route_images(id) ON DELETE CASCADE,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 created_at TEXT NOT NULL DEFAULT (datetime('now')),
 PRIMARY KEY (image_id, user_id)
);
ALTER TABLE route_reports ADD COLUMN image_id TEXT REFERENCES route_images(id) ON DELETE CASCADE;
```

- [ ] **Step 4: Implement the rules, `RouteDraft.photos_open`, `RouteRecord.photos_open`, the saves and the checkbox ("Other riders can add photos", checked by default on upload, reflecting the route on edit). Add `photos_open=1` to the test helper `upload_route` fields.**
- [ ] **Step 5: `cargo test`; all pass.**
- [ ] **Step 6: Commit `feat(photos): schema, access rules and owner contributions switch`.**

### Task 2: Adding photos

**Files:** Modify `src/presentation/web.rs` (handler `add_photos`, route `POST /routes/{slug}/photos`, constants, tests)

**Interfaces:**
- Consumes: `may_add_photo`, `honeypot`, `optimized_image`, `write_asset`, `remove_assets`.
- Produces: `PHOTOS_PER_RIDER_PER_ROUTE: i64 = 5`, `UNVERIFIED_DAILY_PHOTOS: i64 = 10`; test helper `add_photos(app, session, slug, count: usize, website: &str) -> Response` posting synthetic PNGs.

- [ ] **Step 1: Write the failing tests**
  - `riders_add_photos_to_someone_elses_public_route`: second rider adds 2; 303; route page lists both with that rider's username.
  - `photos_refused_on_private_route_or_closed_switch`: other rider → 404 on private, 403 with switch off; owner succeeds on both.
  - `per_route_photo_limit_refuses_whole_submission`: with 4 photos, submitting 3 → 429; still 4 rows; storage file count unchanged.
  - `unverified_daily_photo_limit_holds_under_parallel_requests`: unverified rider spreads 12 single-photo requests in parallel across 3 routes → exactly 10 saved.
  - `photo_submission_needs_a_file_and_no_honeypot`: no files → 400; `website=x` → 400; nothing saved.
- [ ] **Step 2: Run them; expect failures.**
- [ ] **Step 3: Implement `add_photos`.** Check view (404) and `may_add_photo` (403), CSRF, honeypot, captions; write files; in one transaction insert each photo with `INSERT INTO route_images(id,route_id,image_key,position,uploader_id,caption,created_at) SELECT ?,?,?,?,?,?,datetime('now') WHERE (per-route count)<? AND (? OR (daily count)<?)`; any refused row → roll back, remove all written files, 429 naming the limit (per-route message: "You can add up to 5 photos to a route"; daily: "Unverified accounts can add 10 photos a day. Verify your email to remove the limit"). Position = next after the route's max. Success → `msg_redirect` to the route with "Thanks for adding your photos".
- [ ] **Step 4: `cargo test`; all pass.**
- [ ] **Step 5: Commit `feat(photos): riders add photos to shared routes`.**

### Task 3: Route page strip and gallery

**Files:** Modify `src/presentation/web.rs` (`route_detail`, new `route_photos`, `photo_rows` helper), `templates/page.hbs` (strip, add form, gallery page `page_photos`), `static/style.css`.

**Interfaces:**
- Produces: `async fn photo_rows(state: &AppState, r: &RouteRecord, viewer: Option<&User>, filter: PhotoFilter, limit: i64, offset: i64) -> Result<Vec<Value>, AppError>` where `enum PhotoFilter { Owner, Others, All }`; each value has `id, src, username, caption, alt, likes, liked, can_like, can_delete`.

- [ ] **Step 1: Write the failing tests**
  - `route_page_shows_owner_photos_then_six_most_liked`: owner 2 photos, 8 others with distinct like counts (inserted via SQL); page shows owner's 2 first, then the 6 most liked; "See all 10 photos" link present.
  - `gallery_pages_through_all_photos`: 30 photos → page 1 has 24, page 2 has 6 with Previous link.
  - `gallery_of_private_route_is_hidden`: other rider → 404.
- [ ] **Step 2: Run; expect failures.**
- [ ] **Step 3: Implement `photo_rows` (one query joining `users`, like count subquery, `EXISTS` for the viewer's like), the strip and add form on the route page (add form only when `may_add_photo`; signed-out → "Sign in to add your photos"; switch off → "The owner has turned off photo contributions"), and `GET /routes/{slug}/photos?page=` with the explore pagination fields.**
- [ ] **Step 4: `cargo test`; all pass.**
- [ ] **Step 5: Commit `feat(photos): route photo strip and gallery`.**

### Task 4: Likes, deleting and reporting photos

**Files:** Modify `src/presentation/web.rs` (handlers `like_photo`, `unlike_photo`, `delete_photo`, `report_photo`, `safe_back`, `admin`, `admin_report`, `delete_account`, `report_route` validation shared), `templates/page.hbs` (photo actions, admin rows), `static/style.css`.

**Interfaces:**
- Consumes: `photo_rows`, `may_like_photo`, `may_edit`.
- Produces: `fn safe_back(form: &HashMap<String, String>, slug: &str) -> String`; `fn report_fields(form: &HashMap<String, String>) -> Result<(&str, &str), AppError>` used by both report handlers.

- [ ] **Step 1: Write the failing tests**
  - `likes_count_once_and_can_be_removed`: like twice → 1; unlike → 0; liking own photo → 403.
  - `photo_actions_check_the_route_and_visibility`: like/delete/report with a photo id of another route → 404; liking on a private route as another rider → 404.
  - `back_field_cannot_leave_the_route`: `back=//evil.example` and `https://evil.example` redirect to `/routes/{slug}`.
  - `owner_cannot_delete_other_riders_photos`: owner → 403; uploader → 303 and file gone; admin → 303.
  - `admin_removes_reported_photo_without_hiding_route`: report → admin list shows it; `remove` deletes photo and file; route stays public.
  - `deleting_account_removes_photos_on_other_routes`: rider B's photo on A's route and its file are gone.
- [ ] **Step 2: Run; expect failures.**
- [ ] **Step 3: Implement.** Like = `INSERT OR IGNORE`; unlike = `DELETE`; both redirect to `safe_back + #photo-{id}`. Delete removes the row (likes and reports cascade) and the file. Admin `remove` on a report with `image_id` deletes that photo and file; on a route report keeps making the route private. `delete_account` also collects `image_key` where `uploader_id` is the user.
- [ ] **Step 4: `cargo test`; all pass.**
- [ ] **Step 5: Commit `feat(photos): likes, deleting and reporting photos`.**

### Task 5: Docs

**Files:** Modify `docs/roadmap.md`, `docs/architecture.md`, `docs/data-catalog.md`, `docs/contracts.md`, `docs/service-catalog.md`; spec status line.

- [ ] **Step 1: Roadmap: this is Phase 4 (done), Komoot-style becomes Phase 5. Architecture: contributor flow and photo access rules. Data catalog: new columns, `photo_likes`, `route_reports.image_id`, account deletion now removes the rider's photos on other routes. Contracts/service catalog: the new endpoints, if those docs list endpoints. Spec status: done.**
- [ ] **Step 2: `cargo fmt --check && cargo check && cargo test`; all pass.**
- [ ] **Step 3: Commit `docs: community photos`.**
