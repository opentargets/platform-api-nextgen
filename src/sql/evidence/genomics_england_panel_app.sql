(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.allelicRequirements AS allelicRequirements,
    e.studyId AS panelId,
    e.studyOverview AS panelName,
    e.confidence AS confidence,
    e.literature AS literature,
    -- Ascending rating order, so `DESC` puts green first. Unknown and NULL ratings get 0.
    toUInt64(indexOf(['red', 'amber', 'green'], ifNull(e.confidence, ''))) AS confidenceRank
  FROM evidence AS e
)
