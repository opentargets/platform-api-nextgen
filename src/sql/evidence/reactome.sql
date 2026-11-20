(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.targetFromSourceId AS targetFromSourceId,
    e.pathways AS pathways,
    e.reactionId AS reactionId,
    e.reactionName AS reactionName,
    e.targetModulation AS targetModulation,
    e.variantAminoacidDescriptions AS variantAminoacidDescriptions,
    e.literature AS literature
  FROM evidence AS e
)
