-- One-off startup jobs; a row is removed once its job has run.
CREATE TABLE pending_tasks(name TEXT PRIMARY KEY);
INSERT INTO pending_tasks VALUES ('recompute_climbing');
