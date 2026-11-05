-- An evidence is a row (`A`, `B`, `datasourceId`, `Score`).
-- This query runs from inside out, first look at `INNER`.

-- OUTER: runs on each in (`B` * `datasourceId`), produces one row per `B`.
-- Builds the overall association score, and extracts the column being sorted on.
WITH
    -- The maximum value a harmonic sum can have is π²/6 = 1.6449 we use this for normalization to [0, 1].
    {max_hs} AS max_hs_score,
    -- Compute an array of tuples (`datasourceId`, `normalized_score`, `weighted_normalized_score`) and sort by the `weighted_normalized_score` descending.
    arrayReverseSort(x -> x.3, groupArray((datasourceId, datasource_score / max_hs_score, (datasource_score * datasource_weight) / max_hs_score))) AS datasource_scores_pre_hs,
    -- Enumerate `datasource_scores_pre_hs` then computes a harmonic decay and puts it into a tuple (`datasourceId`, `normalized_score`, `weighted_normalized_score / rank²`).
    arrayMap((s, r) -> (s.1, s.2, s.3 / pow(r, 2)), datasource_scores_pre_hs, arrayEnumerate(datasource_scores_pre_hs)) AS datasource_scores_with_hs,
    -- Makes a tuple (`datasourceId`, `normalized_score`), that will become the scores per datasource in each `B` row.
    arrayMap(x -> (x.1, x.2), datasource_scores_with_hs) AS datasource_scores,
    -- Sums the decayed scores of all datasources in a row into the overall score.
    arraySum(datasource_scores_with_hs.3) / max_hs_score AS score,
    -- Grabs novelty.
    any(noveltyWhereA) AS novelty,
    -- If we are ordering by one of the datasources, return its score. If not present, ArrayFirst returns 0.0.
    arrayFirst(x -> x.1 = '{sort_datasource}', datasource_scores).2 AS score_indexed

SELECT
    -- Also returns the total count on every row, so we don't have to query twice to calculate it independently.
    B, score, datasource_scores, novelty, count() OVER () AS total

-- INNER: runs on each `prewhere` row, produces one row per (`B` * `datasourceId`).
-- Groups all evidence rows into a row per (`B` * `datasourceId`), and computes a decaying harmonic sum that becomes that datasource score.
-- Also adds the weights from the policies, and the novelty score.
FROM (
    WITH
        -- Compute scores for every (`B` * `datasource`):
        arraySum(                              -- Lastly, we sum all those scores into a score per datasource per `B`.
            arrayMap((x, y) -> x / pow(y, 2),  -- Then we create an array that contains the harmonic decay per evidence.
                arrayReverseSort(              -- Sorts them descending.
                    groupArray(                -- Gathers all evidence rows for a (`A`, `B`, `datasourceId`)
                        rowScore
                        * if(A = '{a_id}',     1.0, {indirect_w})  -- weights the score down if its an indirect `A`.
                        * if(b_indirect = l.B, 1.0, {indirect_w})  -- weights the score down if its an indirect `B`.
                    )
                ) AS s,
                arrayEnumerate(s)  -- arrayEnumerate(s) just generates a rank (1, 2, 3, 4... of the length of the number of evidence rows).
            )
        ) AS datasource_score,
        mapFromArrays([{datasources}], [{weights}])[datasourceId] AS datasource_weight  -- Adds the policy weigthts for each datasource

    -- The output of this inner query. `b_indirect` becomes `B` again as we've already done the indirect scoring propagation.
    SELECT b_indirect AS B, datasourceId, datasource_score, datasource_weight, anyIf({novelty}, A = '{a_id}') AS noveltyWhereA
    FROM {table} AS l
    ARRAY JOIN
        arrayPushBack(l.indirect, l.B)  -- Build an array of `B`'s indirect ids plus `B` itself.
    AS b_indirect                       -- Explode it into rows with same `A`/`B`/`rowScore`, and a `b_indirect` each from the array.
    WHERE {_where}                      -- Where clause filtering evidence rows at least to those with `A` in our `a_set`.
    GROUP BY b_indirect, datasourceId   -- Group by each indirect, so a parent B absorbs all its descendants' evidence.
)
GROUP BY B
{_having}
ORDER BY {sort_by} {sort_direction}
LIMIT {offset}, {size}
