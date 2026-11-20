(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    -- One entry per mutation type, with the Sequence Ontology label of the consequence.
    arrayMap(
      m -> (
        m.functionalConsequenceId,
        nullIf(
          dictGet(
            'sequence_ontology',
            'label',
            replaceOne(ifNull(m.functionalConsequenceId, ''), '_', ':')
          ),
          ''
        ),
        m.numberMutatedSamples,
        m.numberSamplesTested,
        m.numberSamplesWithMutationType
      ),
      e.mutatedSamples
    ) AS mutatedSamples,
    -- Mutated over total samples. Every mutation type of an evidence carries the same two
    -- totals, so the first one stands for the evidence. Unknown or empty totals give 0.
    ifNull(e.mutatedSamples[1].numberMutatedSamples, 0)
      / greatest(ifNull(e.mutatedSamples[1].numberSamplesTested, 0), 1) AS mutatedSamplesRatio,
    e.literature AS literature
  FROM evidence AS e
)
