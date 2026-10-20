-- Owner-only: the points a rider placed when drawing the route. Null for uploaded GPX.
ALTER TABLE routes ADD COLUMN plan_json TEXT;
