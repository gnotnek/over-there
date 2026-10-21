# Phase 4: Riders add photos to a route

Status: designed, not built. Part of the [roadmap](../roadmap.md).

## Why

A route page today shows only the photos its owner added at upload, at most three. Other riders who ride the same route have no way to show what it looks like, in another season, or from another spot along the way. The goal is that a shared route collects the scenery of everyone who rides it, so anyone opening the page sees what the ride is really like.

User story: rider A shares a route along the river bank and adds a few scenery photos. Rider B rides the same route later and adds their own scenery photos to rider A's route. Anyone viewing the route sees both.

## Goals

- A signed-in rider can add photos to a route someone else shared, if that route is public or unlisted.
- The owner can add more photos to their own route after uploading, including to a private route.
- Each photo shows who added it.
- Riders can like a photo. Likes decide which photos the route page shows, so a popular route stays tidy.
- The route page stays the same size no matter how many photos a route collects. Every photo is reachable on a separate gallery page.
- Whoever added a photo can delete it. Anyone can report a photo to an admin, and an admin can remove it.

## Non-goals

- Pinning photos to a spot on the route map. That stays in the next phase ("Show routes the way Komoot does").
- Comments on photos, or downvotes.
- Notifications to the owner when someone adds a photo.
- A list of a rider's contributed photos on their profile.
- More than one sort order on the gallery page.
- Likes on routes, or using likes in Explore. Likes rank photos within one route only.

## Anti-goals (a failure even if it works)

- The route owner deleting other riders' photos. The user decided owners report, and only an admin removes.
- A photo revealing where someone lives. Photos are re-encoded, which drops camera GPS data. That must stay true for contributed photos.
- One rider or a bot flooding a route or the site with photos.
- A busy route page: the page must not grow with the number of photos.

## Decisions made with the user

- Anyone can view the photos. Adding a photo needs an account.
- Photos can be added to public and unlisted routes; only the owner can add photos to a private route.
- Each photo is labelled with the rider who added it, linked to their profile.
- The route page shows the owner's photos first, then the most-liked photos from other riders. A "See all photos" link opens the full gallery.
- Likes, not up and down votes. One like per rider per photo.
- Each rider can add at most 5 photos to any one route.
- No overall limit on photos per route.
- If the owner doesn't like a photo someone else added, they report it to an admin. They cannot remove it themselves.
- The owner can turn off photo contributions for their route.

## Behaviour

### Adding photos

- Each route has an "Other riders can add photos" switch, on by default. The owner sets it on the upload form, the plan-a-route form and the edit form. When it is off, only the owner can add photos; photos already added stay, and likes and reports keep working. The route page then shows no form to other riders, only a short note that the owner has turned off contributions.
- The route page has an "Add your photos" form for a signed-in rider who may add to that route. Signed-out visitors see a "Sign in to add your photos" link instead.
- One submission adds 1 to 3 photos, each with an optional caption of up to 120 characters. Images are JPEG, PNG or WebP, resized and stored as WebP like today's route photos.
- The form carries the hidden `website` honeypot field and the CSRF token, like the upload and register forms.
- Limits, all enforced in the same statement that saves the photo, so parallel requests cannot get past them (the same approach as the daily route limit):
  - at most 5 photos per rider per route, counting photos the owner added at upload;
  - an account without a verified email can add at most 10 photos per rolling day across all routes.
- When a limit is reached, the rider sees a message saying which one, and no files are left on disk.
- After adding, the rider returns to the route page with a short confirmation.

### Showing photos

- **Route page:** a strip with the owner's photos in their upload order, then the 6 photos from other riders with the most likes. Ties go to the newest photo, so new photos still get a chance. Below it, a "See all N photos" link when there are more photos than the strip shows.
- **Gallery page** (`/routes/{slug}/photos`): every photo of the route, most liked first and newest first among equal likes, 24 per page with Previous and Next.
- Each photo shows the rider's name linked to their profile, its caption if any, its like count, and these actions:
  - a Like or Unlike button for signed-in riders, except on their own photos;
  - Delete, for the rider who added it and for admins;
  - Report, for everyone who can view the route.
- A photo's alt text is its caption, or "Photo of {route title} by {username}" when there is none.
- The gallery and every image follow the route's view rule: if a visitor cannot see the route, they cannot see its photos or the gallery.

### Likes

- Liking works without JavaScript: a small form posts and returns to the same spot on the page.
- Liking again does nothing; Unlike removes the like.
- A rider cannot like their own photo.

### Removing and reporting

- The rider who added a photo, or an admin, can delete it. The file is removed with it.
- The route owner cannot delete other riders' photos.
- Reporting a photo uses the existing report reasons (misleading, copyright, unsafe, other) and appears in the admin report list with the photo shown. The admin can dismiss it, or remove the photo. Removing a photo does not change the route's visibility.
- Deleting a route removes all its photos, including other riders' (as today).
- Deleting an account removes every photo that rider added, on any route, with the files.
- If the owner makes a route private later, other riders' photos stay but are hidden with the route. Their authors cannot see or delete them until the route is visible again.

## Data

- `route_images` gains:
  - `uploader_id`, referencing `users`, removing the row when the user is deleted;
  - `caption`, text, empty by default;
  - `created_at`.
  Existing rows are backfilled with the route's author as `uploader_id`. SQLite cannot add a column with a `datetime('now')` default through `ALTER TABLE`, so either rebuild the table in the migration or add `created_at` without a default and always set it on insert.
- New table `photo_likes(image_id, user_id, created_at)`, primary key `(image_id, user_id)`, both columns removing the row when the photo or user is deleted. Index by `image_id` for counting.
- `routes` gains `photos_open`, an integer flag, 1 by default, so existing routes accept contributions.
- `route_reports` gains a nullable `image_id` referencing `route_images`, removing the report when the photo is deleted. A report with `image_id` set is about that photo; without it, about the route.
- New migration after the latest one on `main`. Update `docs/data-catalog.md` for every new column and table.

## Domain

- The rules for who may add, delete and like belong in `src/domain/route.rs`, next to `may_view` and `may_edit`, with focused tests:
  - may add: the rider is the owner, or the route is public or unlisted and `photos_open` is on;
  - may delete: the rider who added the photo, or an admin;
  - may like: may view the route, and is not the photo's author.
- The two limits (5 per rider per route, 10 per day for unverified accounts) are constants next to `UNVERIFIED_DAILY_ROUTES`.

## Routes

- `POST /routes/{slug}/photos`: add photos.
- `GET /routes/{slug}/photos`: gallery, with `?page=`.
- `POST /routes/{slug}/photos/{id}/like` and `POST /routes/{slug}/photos/{id}/unlike`.
- `POST /routes/{slug}/photos/{id}/delete`.
- `POST /routes/{slug}/photos/{id}/report`.
- The existing `GET /routes/{slug}/images/{id}` keeps serving every photo, owner's or not.

## Browser

No new JavaScript. Forms post and redirect, like the rest of the site. Styles follow the palette in `docs/architecture.md`; the like button uses the lime accent when the viewer has liked the photo.

## Docs to update

- `docs/roadmap.md`: add this as Phase 4 and move "Show routes the way Komoot does" to Phase 5.
- `docs/architecture.md`: the contributor flow and the photo access rules.
- `docs/data-catalog.md`: the new columns and table, and that account deletion now also removes photos on other riders' routes.
- `docs/contracts.md` and `docs/service-catalog.md` if they list endpoints.

## Tests

- Domain: may add, delete and like, covering owner, other rider, admin, signed out, and each visibility.
- A rider adds photos to someone else's public route; they show on the route page with the rider's name.
- Adding to someone else's private route is refused; the owner can add to their own private route.
- With contributions turned off, another rider's photo is refused and the form is hidden; the owner can still add, and turning it back on lets others add again. The edit form saves the switch.
- The 6th photo from one rider on one route is refused, and leaves no file behind.
- An unverified account's 11th photo in a day is refused; parallel requests cannot get past either limit.
- The honeypot rejects a photo submission.
- The route page shows the owner's photos, then the 6 most-liked others; the gallery pages through the rest.
- Liking twice counts once; unliking removes it; liking your own photo is refused.
- The route owner cannot delete another rider's photo; its author and an admin can.
- Reporting a photo shows it in the admin list; the admin removing it deletes the photo and file, and leaves the route's visibility unchanged.
- Deleting an account removes that rider's photos on other routes, with the files.
- Photos and the gallery of a private route return 404 to other riders.

## Left to the builder

- Table rebuild or plain columns in the migration.
- Markup and layout of the strip, gallery and like button, within the palette.
- How the admin report list shows photo reports next to route reports.
- Exact confirmation and limit messages.

## Calls I made (not discussed with the user)

- Owners are also limited to 5 photos per route, counting their upload photos.
- Unverified accounts can add 10 photos per day across all routes.
- 1 to 3 photos per submission, captions up to 120 characters.
- The route page strip shows 6 photos from other riders; the gallery shows 24 per page.
- Riders cannot like their own photos.
- Likes, then newest, is the only order on both the strip and the gallery.
- Other riders' photos stay hidden, not deleted, when the owner makes a route private.
- A deleted photo frees its slot in the 5-per-route limit. Reports on photos work for signed-out visitors too, as route reports do today.
- This becomes Phase 4, and the Komoot-style phase moves to Phase 5.
- The contributions switch is on by default, for new and existing routes.
