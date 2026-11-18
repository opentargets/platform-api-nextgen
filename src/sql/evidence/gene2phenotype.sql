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
    e.allelicRequirements AS allelicRequirements,
    e.studyId AS studyId,
    e.confidence AS confidence,
    e.literature AS literature
  FROM evidence AS e
)
