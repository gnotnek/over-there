-- Bounding box of the public (endpoint-trimmed) geometry, used to prefilter nearby searches.
ALTER TABLE routes ADD COLUMN min_lat REAL;
ALTER TABLE routes ADD COLUMN max_lat REAL;
ALTER TABLE routes ADD COLUMN min_lon REAL;
ALTER TABLE routes ADD COLUMN max_lon REAL;
CREATE INDEX routes_bbox_idx ON routes(visibility, min_lat, max_lat);
