(
  SELECT
    c.credibleSetId AS credibleSetId,
    l.variantId AS variantId,
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
    credible_sets_locus AS c
    ARRAY JOIN c.locus AS l
    -- A key lookup on the EmbeddedRocksDB variants table.
    LEFT JOIN variants AS v ON v.variantId = l.variantId
)
