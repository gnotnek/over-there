PRAGMA foreign_keys = ON;
CREATE TABLE users (
 id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE,
 email TEXT NOT NULL UNIQUE COLLATE NOCASE, password_hash TEXT NOT NULL,
 bio TEXT NOT NULL DEFAULT '', avatar_key TEXT,
 email_verified_at TEXT, contributor_verified_at TEXT, is_admin INTEGER NOT NULL DEFAULT 0,
 created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE sessions (
 token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 csrf_token TEXT NOT NULL, expires_at TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX sessions_user_idx ON sessions(user_id);
CREATE TABLE auth_attempts (
 email TEXT PRIMARY KEY, failures INTEGER NOT NULL, window_started_at TEXT NOT NULL
);
CREATE TABLE email_verifications (
 token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 expires_at TEXT NOT NULL, sent_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE email_send_quota (
 day TEXT PRIMARY KEY, sent INTEGER NOT NULL
);
CREATE TABLE routes (
 id TEXT PRIMARY KEY, author_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 slug TEXT NOT NULL UNIQUE, title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
 activity TEXT NOT NULL DEFAULT 'cycling', cycling_type TEXT NOT NULL,
 region TEXT NOT NULL, country TEXT NOT NULL, difficulty TEXT NOT NULL,
 tags TEXT NOT NULL DEFAULT '', visibility TEXT NOT NULL CHECK (visibility IN ('public','unlisted','private')),
 gpx_key TEXT NOT NULL, public_gpx_key TEXT, gpx_sha256 TEXT NOT NULL, hide_endpoints_m REAL NOT NULL DEFAULT 0,
 geometry_json TEXT NOT NULL, elevation_json TEXT,
 distance_m REAL NOT NULL, elevation_gain_m REAL, elevation_loss_m REAL, duration_s INTEGER,
 download_count INTEGER NOT NULL DEFAULT 0, cover_key TEXT,
 created_at TEXT NOT NULL DEFAULT (datetime('now')),
 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX routes_discover_idx ON routes(visibility, created_at DESC);
CREATE INDEX routes_author_idx ON routes(author_id, created_at DESC);
CREATE INDEX routes_search_idx ON routes(visibility, cycling_type, difficulty, distance_m);
CREATE INDEX routes_duplicate_idx ON routes(author_id, gpx_sha256);
CREATE TABLE route_images (
 id TEXT PRIMARY KEY, route_id TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
 image_key TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE route_reports (
 id TEXT PRIMARY KEY, route_id TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
 reporter_id TEXT REFERENCES users(id) ON DELETE SET NULL,
 reason TEXT NOT NULL, details TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'open',
 created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
