-- Riders add photos to routes others shared; the owner can turn contributions off.
ALTER TABLE routes ADD COLUMN photos_open INTEGER NOT NULL DEFAULT 1;
ALTER TABLE route_images ADD COLUMN uploader_id TEXT REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE route_images ADD COLUMN caption TEXT NOT NULL DEFAULT '';
-- No datetime('now') default: ALTER TABLE cannot add one, so every insert sets it.
ALTER TABLE route_images ADD COLUMN created_at TEXT;
UPDATE route_images SET
 uploader_id=(SELECT author_id FROM routes WHERE routes.id=route_images.route_id),
 created_at=(SELECT created_at FROM routes WHERE routes.id=route_images.route_id);
CREATE INDEX route_images_uploader_idx ON route_images(uploader_id, created_at);
CREATE TABLE photo_likes (
 image_id TEXT NOT NULL REFERENCES route_images(id) ON DELETE CASCADE,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 created_at TEXT NOT NULL DEFAULT (datetime('now')),
 PRIMARY KEY (image_id, user_id)
);
ALTER TABLE route_reports ADD COLUMN image_id TEXT REFERENCES route_images(id) ON DELETE CASCADE;
