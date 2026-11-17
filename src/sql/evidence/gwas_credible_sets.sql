(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.targetId AS targetId,
    e.studyLocusId AS studyLocusId,
    c.variantId AS leadVariantId,
    c.chromosome AS leadChromosome,
    c.position AS leadPosition,
    -- Variant ids are `chromosome_position_reference_alternate`.
    splitByChar('_', c.variantId)[3] AS leadReferenceAllele,
    splitByChar('_', c.variantId)[4] AS leadAlternateAllele,
    c.pValueMantissa AS pValueMantissa,
    c.pValueExponent AS pValueExponent,
    c.beta AS beta,
    c.finemappingMethod AS finemappingMethod,
    c.confidence AS confidence,
    -- Ascending confidence order, so `DESC` puts the best fine-mapping first. Unknown values get 0.
    toUInt64(indexOf(
      [
        'PICS fine-mapped credible set based on reported top hit',
        'PICS fine-mapped credible set extracted from summary statistics',
        'SuSiE fine-mapped credible set with out-of-sample LD',
        'SuSiE fine-mapped credible set with in-sample LD'
      ],
      toString(c.confidence)
    )) AS confidenceRank,
    c.studyId AS studyId,
    s.traitFromSource AS traitFromSource,
    s.projectId AS projectId,
    s.publicationFirstAuthor AS publicationFirstAuthor,
    s.publicationDate AS publicationDate,
    s.pubmedId AS pubmedId,
    s.nSamples AS nSamples
  FROM evidence AS e
  LEFT JOIN credible_sets AS c ON c.studyLocusId = ifNull(e.studyLocusId, '')
  LEFT JOIN studies AS s ON s.studyId = c.studyId
)
