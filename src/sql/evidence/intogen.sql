(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
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
    e.resourceScore AS resourceScore,
    e.significantDriverMethods AS significantDriverMethods,
    e.cohortId AS cohortId,
    e.cohortShortName AS cohortShortName,
    e.cohortDescription AS cohortDescription
  FROM evidence AS e
)
