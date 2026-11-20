(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    t.approvedSymbol AS targetApprovedSymbol,
    t.approvedName AS targetApprovedName,
    e.variantFunctionalConsequenceId AS variantFunctionalConsequenceId,
    nullIf(
      dictGet(
        'sequence_ontology',
        'label',
        replaceOne(ifNull(e.variantFunctionalConsequenceId, ''), '_', ':')
      ),
      ''
    ) AS variantFunctionalConsequenceLabel,
    e.directionOnTarget AS directionOnTarget,
    e.directionOnTrait AS directionOnTrait,
    e.alleleOrigins AS alleleOrigins,
    e.confidence AS confidence,
    e.literature AS literature
  FROM evidence AS e
  LEFT JOIN targets AS t ON t.id = e.targetId
)
