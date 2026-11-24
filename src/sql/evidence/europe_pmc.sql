(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.literature AS literature,
    e.pmcIds AS pmcIds,
    e.publicationYear AS publicationYear,
    e.textMiningSentences AS textMiningSentences,
    e.resourceScore AS resourceScore
  FROM evidence AS e
)
