(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    e.variantId AS variantId,
    if(v.variantId = '', NULL, v.chromosome) AS variantChromosome,
    if(v.variantId = '', NULL, v.position) AS variantPosition,
    if(v.variantId = '', NULL, v.referenceAllele) AS variantReferenceAllele,
    if(v.variantId = '', NULL, v.alternateAllele) AS variantAlternateAllele,
    v.hgvsId AS variantHgvsId,
    e.variantRsId AS variantRsId,
    e.studyId AS studyId,
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
    e.clinicalSignificances AS clinicalSignificances,
    e.allelicRequirements AS allelicRequirements,
    e.alleleOrigins AS alleleOrigins,
    e.confidence AS confidence,
    e.literature AS literature,
    e.cohortPhenotypes AS cohortPhenotypes
  FROM evidence AS e
  LEFT JOIN variants AS v ON v.variantId = ifNull(e.variantId, '')
)
