(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.studyId AS studyId,
    e.studyOverview AS studyOverview,
    e.contrast AS contrast,
    e.confidence AS confidence,
    e.log2FoldChangePercentileRank AS log2FoldChangePercentileRank,
    e.resourceScore AS resourceScore
  FROM evidence AS e
)
