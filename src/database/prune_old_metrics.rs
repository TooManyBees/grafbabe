use rusqlite::Connection;

pub fn prune_old_metrics(connection: &Connection) -> rusqlite::Result<usize> {
    let mut statement = connection.prepare(
        "DELETE FROM events
        WHERE timestamp < (unixepoch('now') * 1000 - ?);",
    )?;

    const ONE_MONTH_MILLIS: i64 = 1000 * 60 * 60 * 24 * 30;

    statement.execute((ONE_MONTH_MILLIS,))
}
