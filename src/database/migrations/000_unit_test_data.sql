INSERT INTO metrics (id, name, kind, help)
VALUES (1, 'some_metric',    0, 'A counter'),
       (2, 'another_metric', 1, 'A gauge'),
       (3, 'mystery_metric', 0, NULL);

INSERT INTO labels (id, label)
VALUES (1, 'a_label');

INSERT INTO events (id, timestamp)
VALUES (1, 1786369087170);

INSERT INTO metric_values (metric_id, label_id, event_id, value)
VALUES (1,    1, 1, 10),
       (2,    1, 1, 20),
       (3, NULL, 1, 30);
