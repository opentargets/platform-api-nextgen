(
  SELECT
    c.credibleSetId AS credibleSetId,
    l.variantId AS variantId,
    toBool(l.variantId = c.leadVariantId) AS isLead,
    v.chromosome AS chromosome,
    v.position AS position,
    v.referenceAllele AS referenceAllele,
    v.alternateAllele AS alternateAllele,
    l.posteriorProbability AS posteriorProbability,
    l.logBayesFactor AS logBayesFactor,
    l.pValueMantissa AS pValueMantissa,
    l.pValueExponent AS pValueExponent,
    l.reportedBeta AS reportedBeta,
    l.reportedStandardError AS reportedStandardError,
    l.r2Overall AS r2Overall,
    l.sampleSize AS sampleSize,
    l.effectAlleleFrequency AS effectAlleleFrequency,
    l.conditionalBeta AS conditionalBeta,
    l.conditionalStandardError AS conditionalStandardError,
    l.rescaledAbsBeta AS rescaledAbsBeta,
    l.rescaledStandardError AS rescaledStandardError,
    l.rescaledDirection AS rescaledDirection,
    -- Variants use SO_ ids and the ontology uses SO: ids, like the legacy field.
    nullIf(replaceOne(v.mostSevereConsequenceId, '_', ':'), '') AS mostSevereConsequenceId,
    nullIf(
      dictGet(
        'sequence_ontology',
        'label',
        replaceOne(v.mostSevereConsequenceId, '_', ':')
      ),
      ''
    ) AS mostSevereConsequenceLabel
  FROM
    (
      SELECT
        cl.credibleSetId AS credibleSetId,
        cl.locus AS locus,
        cs.leadVariantId AS leadVariantId
      FROM
        credible_sets_locus AS cl
        LEFT JOIN credible_sets AS cs ON cs.credibleSetId = cl.credibleSetId
    ) AS c
    ARRAY JOIN c.locus AS l
    -- A key lookup on the EmbeddedRocksDB variants table.
    LEFT JOIN variants AS v ON v.variantId = l.variantId
)
