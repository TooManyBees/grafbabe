CREATE TABLE metrics (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    kind INTEGER NOT NULL DEFAULT 0,
    help TEXT
) STRICT;

CREATE UNIQUE INDEX metrics_by_name ON metrics(name);

CREATE TABLE labels (
    id INTEGER PRIMARY KEY,
    label TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX labels_by_label ON labels(label);

CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    timestamp INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX events_by_timestamp ON events(timestamp);

CREATE TABLE metric_values (
    metric_id INTEGER NOT NULL REFERENCES metrics(id) ON DELETE CASCADE,
    label_id INTEGER REFERENCES labels(id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    value REAL NOT NULL
) STRICT;

CREATE INDEX metric_values_by_event_id ON metric_values(event_id);

CREATE TABLE grafbabe_migrations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
) STRICT;
