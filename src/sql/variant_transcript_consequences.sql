(
  SELECT
    v.variantId AS variantId,
    tc.variantFunctionalConsequenceIds AS variantFunctionalConsequenceIds,
    tc.aminoAcidChange AS aminoAcidChange,
    tc.uniprotAccessions AS uniprotAccessions,
    tc.isEnsemblCanonical AS isEnsemblCanonical,
    tc.codons AS codons,
    tc.distanceFromFootprint AS distanceFromFootprint,
    tc.distanceFromTss AS distanceFromTss,
    tc.targetId AS targetId,
    tc.impact AS impact,
    tc.transcriptId AS transcriptId,
    tc.lofteePrediction AS lofteePrediction,
    tc.transcriptIndex AS transcriptIndex,
    tc.consequenceScore AS consequenceScore,
    nullIf(
      dictGet(
        'target_symbols',
        'approvedSymbol',
        ifNull(tc.targetId, '')
      ),
      ''
    ) AS targetApprovedSymbol,
    arrayMap(
      i -> dictGet(
        'sequence_ontology',
        'label',
        replaceOne(i, '_', ':')
      ),
      tc.variantFunctionalConsequenceIds
    ) AS variantConsequenceLabels
  FROM
    variants AS v
    ARRAY JOIN v.transcriptConsequences AS tc
)
