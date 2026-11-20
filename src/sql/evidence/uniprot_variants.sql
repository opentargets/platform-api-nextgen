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
    e.variantId AS variantId,
    v.hgvsId AS variantHgvsId,
    e.variantRsId AS variantRsId,
    -- amino acid change on this evidence's own target, not on the variant's other genes
    arrayFirst(
      t -> ifNull(t.targetId, '') = e.targetId,
      v.transcriptConsequences
    ).aminoAcidChange AS variantAminoAcidChange,
    nullIf(v.mostSevereConsequenceId, '') AS variantConsequenceId,
    nullIf(
      dictGet(
        'sequence_ontology',
        'label',
        replaceOne(v.mostSevereConsequenceId, '_', ':')
      ),
      ''
    ) AS variantConsequenceLabel,
    e.confidence AS confidence,
    e.literature AS literature
  FROM evidence AS e
  LEFT JOIN variants AS v ON v.variantId = ifNull(e.variantId, '')
)
