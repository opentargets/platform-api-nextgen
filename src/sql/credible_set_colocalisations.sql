(
  SELECT
    c.studyLocusId AS studyLocusId,
    x.otherStudyLocusId AS otherStudyLocusId,
    x.rightStudyType AS studyType,
    cs.studyId AS studyId,
    s.projectId AS projectId,
    s.traitFromSource AS traitFromSource,
    s.publicationFirstAuthor AS publicationFirstAuthor,
    s.condition AS condition,
    s.geneId AS targetId,
    nullIf(
      dictGet('target_symbols', 'approvedSymbol', ifNull(s.geneId, '')),
      ''
    ) AS targetApprovedSymbol,
    s.biosampleFromSourceId AS biosampleId,
    nullIf(b.biosampleName, '') AS biosampleName,
    b.description AS biosampleDescription,
    cs.isTransQtl AS isTransQtl,
    cs.variantId AS leadVariantId,
    cs.chromosome AS leadChromosome,
    cs.position AS leadPosition,
    cs.pValueMantissa AS pValueMantissa,
    cs.pValueExponent AS pValueExponent,
    x.colocalisationMethod AS colocalisationMethod,
    x.numberColocalisingVariants AS numberColocalisingVariants,
    x.h3 AS h3,
    x.h4 AS h4,
    x.clpp AS clpp,
    x.betaRatioSignAverage AS betaRatioSignAverage
  FROM
    colocalisation AS c
    ARRAY JOIN c.colocalisation AS x
    -- All three joins are key lookups on EmbeddedRocksDB tables.
    LEFT JOIN credible_sets AS cs ON cs.studyLocusId = x.otherStudyLocusId
    LEFT JOIN studies AS s ON s.studyId = cs.studyId
    LEFT JOIN biosample AS b ON b.biosampleId = s.biosampleFromSourceId
)
