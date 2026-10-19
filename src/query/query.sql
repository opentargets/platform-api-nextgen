WITH
    ? AS q_offset,
    ? AS q_limit,
    CAST(? AS UInt64) AS query_id,
    filtered AS
    (
        SELECT *
        FROM {{table}}
        WHERE {{where}}
    ),
    (SELECT count() FROM filtered) AS total,
    paged AS
    (
        SELECT *
        FROM filtered
        ORDER BY {{order_by}}
        LIMIT q_limit OFFSET q_offset
    )
SELECT
    query_id,
    tuple(
        CAST(total AS UInt64),
        groupArray(tuple(*)) AS rows
    )
FROM paged
GROUP BY query_id
