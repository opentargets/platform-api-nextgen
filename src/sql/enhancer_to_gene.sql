(
  -- Columns listed explicitly, in the same order as `EnhancerToGene`: rows are
  -- decoded by position. Listing them (rather than `e.*`) also avoids name clashes
  -- with the joined `biosample` table, which has its own `biosampleId`/`biosampleName`.
  SELECT
    e.chromosome AS chromosome,
    e.start AS start,
    e.`end` AS `end`,
    e.geneId AS geneId,
    e.biosampleName AS biosampleName,
    e.biosampleId AS biosampleId,
    e.biosampleFromSourceId AS biosampleFromSourceId,
    e.intervalType AS intervalType,
    e.distanceToTss AS distanceToTss,
    e.score AS score,
    e.resourceScore AS resourceScore,
    e.datasourceId AS datasourceId,
    e.pmid AS pmid,
    e.studyId AS studyId,
    e.qualityControls AS qualityControls,
    -- Joined in for sort and search only. `nullIf` turns unmatched joins ('') into
    -- NULL so they sort last.
    nullIf(t.approvedSymbol, '') AS targetApprovedSymbol,
    -- The biosample table's name, which is what the API returns as
    -- `biosample.biosampleName`. It can differ from `e.biosampleName`.
    nullIf(b.biosampleName, '') AS biosampleDisplayName
  FROM
    enhancer_to_gene AS e
    -- Plain table joins (no subqueries) so ClickHouse looks up each key directly
    -- instead of reading the whole `targets`/`biosample` table.
    LEFT JOIN targets AS t ON e.geneId = t.id
    LEFT JOIN biosample AS b ON toString(e.biosampleId) = b.biosampleId
)
