use rusqlite::Connection;

pub fn init_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "CREATE TABLE IF NOT EXISTS metrics (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            kind INTEGER NOT NULL DEFAULT 0,
            help TEXT
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS metrics_by_name ON metrics(name);",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS labels (
            id INTEGER PRIMARY KEY,
            label TEXT NOT NULL
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS labels_by_label ON labels(label);",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            timestamp INTEGER NOT NULL
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS events_by_timestamp ON events(timestamp);",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS metric_values (
            metric_id INTEGER NOT NULL REFERENCES metrics(id) ON DELETE CASCADE,
            label_id INTEGER REFERENCES labels(id) ON DELETE CASCADE,
            event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            value INTEGER NOT NULL
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS metric_values_by_event_id ON metric_values(event_id);",
        (),
    )?;

    Ok(())
}
