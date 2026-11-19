(
  SELECT
    c.studyLocusId AS studyLocusId,
    l.variantId AS variantId,
    v.chromosome AS chromosome,
    v.position AS position,
    v.referenceAllele AS referenceAllele,
    v.alternateAllele AS alternateAllele,
    l.posteriorProbability AS posteriorProbability,
    l.logBF AS logBF,
    l.pValueMantissa AS pValueMantissa,
    l.pValueExponent AS pValueExponent,
    l.beta AS beta,
    l.standardError AS standardError,
    l.r2Overall AS r2Overall,
    nullIf(replaceOne(v.mostSevereConsequenceId, '_', ':'), '') AS mostSevereConsequenceId,
    nullIf(so.label, '') AS mostSevereConsequenceLabel
  FROM
    credible_sets_locus AS c
    ARRAY JOIN c.locus AS l
    LEFT JOIN variants AS v ON v.variantId = l.variantId
    LEFT JOIN sequence_ontology AS so
      ON so.id = replaceOne(v.mostSevereConsequenceId, '_', ':')
)
