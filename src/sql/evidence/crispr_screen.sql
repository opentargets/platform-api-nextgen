(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.diseaseFromSourceMappedId AS diseaseFromSourceMappedId,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.targetFromSourceId AS targetFromSourceId,
    e.datasourceId AS datasourceId,
    e.datatypeId AS datatypeId,
    e.projectId AS projectId,
    e.studyId AS studyId,
    e.studyOverview AS studyOverview,
    e.contrast AS contrast,
    e.cellType AS cellType,
    e.crisprScreenLibrary AS crisprScreenLibrary,
    e.statisticalTestTail AS statisticalTestTail,
    e.phenotypicConsequenceScore AS phenotypicConsequenceScore,
    e.resourceScore AS resourceScore,
    e.literature AS literature
  FROM evidence AS e
)
