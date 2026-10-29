(
  SELECT
    p.*,
    -- `nullIf` turns unmatched joins ('') into NULL so they sort last.
    if(
      p.targetFromSourceId IS NULL,
      NULL,
      nullIf(t.approvedSymbol, '')
    ) AS targetApprovedSymbol,
    if(
      p.variantFunctionalConsequenceId IS NULL,
      NULL,
      nullIf(toString(so.label), '')
    ) AS variantFunctionalConsequenceLabel,
    arrayMap(d -> toString(d.drugId), p.drugs) AS drugIds,
    arrayMap(d -> toString(d.drugFromSource), p.drugs) AS drugFromSources
  FROM
    pharmacogenomics AS p
    LEFT JOIN targets AS t ON ifNull(p.targetFromSourceId, '') = t.id
    LEFT JOIN sequence_ontology AS so ON toLowCardinality(
      replaceOne(
        ifNull(p.variantFunctionalConsequenceId, ''),
        '_',
        ':'
      )
    ) = so.id
)
