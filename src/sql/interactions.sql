(
    -- One row per interaction of targetA. The outer targetA filter is pushed down to an
    -- EmbeddedRocksDB key lookup, so only that target's array is read and unnested.
    -- idx is the position in the array: interactions can repeat exactly, so it's the tie breaker.
    SELECT
        t.targetA AS targetA,
        idx AS idx,
        i.intA AS intA,
        i.targetB AS targetB,
        i.intB AS intB,
        i.intABiologicalRole AS intABiologicalRole,
        i.intBBiologicalRole AS intBBiologicalRole,
        i.scoring AS score,
        i.count AS count,
        i.sourceDatabase AS sourceDatabase,
        i.speciesA AS speciesA,
        i.speciesB AS speciesB,
        i.evidences AS evidences
    FROM interaction AS t
    ARRAY JOIN t.interactions AS i, arrayEnumerate(t.interactions) AS idx
)
