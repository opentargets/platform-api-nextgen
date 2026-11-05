(
  -- One row per transcript consequence, unpacked from `variants.transcriptConsequences`.
  -- Columns are listed in the same order as `VariantTranscriptConsequence`:
  -- rows are decoded by position.
  --
  -- `variants` is keyed on `variantId`, so the `variantId` filter added from Rust
  -- reads only that variant. The gene symbol and consequence labels are joined in
  -- for sort and search only.
  WITH
    -- `SO:` id -> label, e.g. 'SO:0001627' -> 'intron_variant'. A small table (~2.6k rows).
    (
      SELECT
        CAST(
          (groupArray(toString(id)), groupArray(toString(label))),
          'Map(String, String)'
        )
      FROM
        sequence_ontology
    ) AS so_labels
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
    tc.siftPrediction AS siftPrediction,
    tc.polyphenPrediction AS polyphenPrediction,
    tc.transcriptIndex AS transcriptIndex,
    tc.consequenceScore AS consequenceScore,
    -- `nullIf` turns unmatched joins ('') into NULL so they sort last.
    if(
      tc.targetId IS NULL,
      NULL,
      nullIf(t.approvedSymbol, '')
    ) AS targetApprovedSymbol,
    -- Ids here use `_` (SO_0001627), the ontology table uses `:` (SO:0001627).
    arrayMap(
      i -> so_labels[replaceOne(i, '_', ':')],
      tc.variantFunctionalConsequenceIds
    ) AS variantConsequenceLabels
  FROM
    variants AS v
    ARRAY JOIN v.transcriptConsequences AS tc
    LEFT JOIN targets AS t ON ifNull(tc.targetId, '') = t.id
)
