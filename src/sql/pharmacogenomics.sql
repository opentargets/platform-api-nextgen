(
  SELECT
    p.*,
    nullIf(
      dictGet(
        'target_symbols',
        'approvedSymbol',
        ifNull(p.targetFromSourceId, '')
      ),
      ''
    ) AS targetApprovedSymbol,
    nullIf(
      dictGet(
        'sequence_ontology',
        'label',
        replaceOne(
          ifNull(p.variantFunctionalConsequenceId, ''),
          '_',
          ':'
        )
      ),
      ''
    ) AS variantFunctionalConsequenceLabel,
    arrayMap(d -> toString(d.drugId), p.drugs) AS drugIds,
    arrayMap(d -> toString(d.drugFromSource), p.drugs) AS drugFromSources
  FROM
    pharmacogenomics AS p
)
