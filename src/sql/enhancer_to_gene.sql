(
  -- Columns listed explicitly, in the same order as `EnhancerToGene`: rows are
  -- decoded by position.
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
    nullIf(
      dictGet('target_symbols', 'approvedSymbol', e.geneId),
      ''
    ) AS targetApprovedSymbol,
    nullIf(
      dictGet(
        'biosample_names',
        'name',
        toString(e.biosampleId)
      ),
      ''
    ) AS biosampleDisplayName
  FROM
    enhancer_to_gene AS e
)
