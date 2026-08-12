ALTER TABLE metric_values
ADD COLUMN value_f REAL;

UPDATE metric_values
SET value_f = CAST(value AS REAL);

-- ALTER TABLE ALTER COLUMN requires sqlite3 3.53 (2026-04-09)
ALTER TABLE metric_values
ALTER COLUMN value_f SET NOT NULL;

ALTER TABLE metric_values
DROP COLUMN value;

ALTER TABLE metric_values
RENAME COLUMN value_f TO value;
