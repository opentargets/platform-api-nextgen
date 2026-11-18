(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.targetId AS targetId,
    e.studyLocusId AS credibleSetId,
    c.leadVariantId AS leadVariantId,
    c.chromosome AS leadChromosome,
    c.leadPosition AS leadPosition,
    -- Variant ids are `chromosome_position_reference_alternate`.
    splitByChar('_', c.leadVariantId)[3] AS leadReferenceAllele,
    splitByChar('_', c.leadVariantId)[4] AS leadAlternateAllele,
    c.leadPValueMantissa AS pValueMantissa,
    c.leadPValueExponent AS pValueExponent,
    c.leadBeta AS beta,
    c.fineMappingMethod AS fineMappingMethod,
    c.confidence AS confidence,
    c.studyId AS studyId,
    s.traitFromSource AS traitFromSource,
    s.projectId AS projectId,
    s.publicationFirstAuthor AS publicationFirstAuthor,
    s.publicationDate AS publicationDate,
    s.pubmedId AS pubmedId,
    s.nSamples AS nSamples
  FROM evidence AS e
  LEFT JOIN credible_sets AS c ON c.credibleSetId = ifNull(e.studyLocusId, '')
  LEFT JOIN studies AS s ON s.studyId = c.studyId
)
