(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.biomarkerName AS biomarkerName,
    arrayMap(
      v -> (
        v.id,
        v.name,
        v.functionalConsequenceId,
        nullIf(
          dictGet(
            'sequence_ontology',
            'label',
            replaceOne(ifNull(v.functionalConsequenceId, ''), '_', ':')
          ),
          ''
        )
      ),
      e.biomarkers.geneticVariation
    ) AS biomarkerGeneticVariations,
    -- GO labels come from key lookups on gene_ontology, one per element. The data holds at most
    -- two terms per evidence; a third would get no label.
    arrayMap(
      (g, i) -> (g.name, g.id, nullIf([go1.label, go2.label][i], '')),
      e.biomarkers.geneExpression,
      arrayEnumerate(e.biomarkers.geneExpression)
    ) AS biomarkerGeneExpressions,
    e.drugId AS drugId,
    nullIf(d.name, '') AS drugName,
    e.drugFromSource AS drugFromSource,
    e.drugResponse AS drugResponseId,
    -- Drug responses are disease terms (e.g. drug resistance) or GO terms.
    coalesce(
      nullIf(dictGet('disease_names', 'name', ifNull(e.drugResponse, '')), ''),
      nullIf(gor.label, '')
    ) AS drugResponseName,
    e.confidence AS confidence,
    e.literature AS literature,
    e.urls AS urls
  FROM
    evidence AS e
    LEFT JOIN drug AS d ON d.id = ifNull(e.drugId, '')
    LEFT JOIN gene_ontology AS gor ON gor.id = replaceOne(ifNull(e.drugResponse, ''), '_', ':')
    LEFT JOIN gene_ontology AS go1
      ON go1.id = replaceOne(ifNull(e.biomarkers.geneExpression[1].id, ''), '_', ':')
    LEFT JOIN gene_ontology AS go2
      ON go2.id = replaceOne(ifNull(e.biomarkers.geneExpression[2].id, ''), '_', ':')
)
