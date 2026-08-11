use rusqlite::Connection;

pub fn prune_old_metrics(connection: &Connection) -> rusqlite::Result<usize> {
    let mut statement = connection.prepare(
        "DELETE FROM events
        WHERE timestamp < (unixepoch('now') * 1000 - ?);",
    )?;

    const ONE_MONTH_MILLIS: i64 = 1000 * 60 * 60 * 24 * 30;

    let dropped = statement.execute((ONE_MONTH_MILLIS,))?;

    log::debug!(
        "Dropped {dropped} old {}",
        if dropped == 1 { "event" } else { "events" },
    );

    Ok(dropped)
}

pub fn prune_unused_metrics(connection: &Connection) -> rusqlite::Result<(usize, usize)> {
    let dropped_metrics = {
        let mut statement = connection.prepare(
            "DELETE FROM metrics
            WHERE ROWID IN (
                SELECT metrics.ROWID
                FROM metrics LEFT JOIN metric_values
                ON metrics.id = metric_values.metric_id
                WHERE metric_values.metric_id IS NULL
            );",
        )?;
        statement.execute(())?
    };

    if dropped_metrics > 0 {
        log::info!(
            "Dropped {dropped_metrics} unused {}",
            if dropped_metrics == 1 {
                "metric"
            } else {
                "metrics"
            },
        );
    }

    let dropped_labels = {
        let mut statement = connection.prepare(
            "DELETE FROM labels
            WHERE ROWID IN (
                SELECT labels.ROWID
                FROM labels LEFT JOIN metric_values
                ON labels.id = metric_values.label_id
                WHERE metric_values.label_id IS NULL
            );",
        )?;
        statement.execute(())?
    };

    if dropped_labels > 0 {
        log::info!(
            "Dropped {dropped_labels} unused {}",
            if dropped_labels == 1 {
                "label"
            } else {
                "labels"
            }
        );
    }

    Ok((dropped_metrics, dropped_labels))
}
