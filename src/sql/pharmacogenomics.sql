(
  -- Columns listed explicitly, in the same order as `Pharmacogenomics`: rows are
  -- decoded by position.
  SELECT
    p.pharmacogenomicsId AS rowId,
    p.datasourceId AS datasourceId,
    p.datatypeId AS datatypeId,
    p.drugs AS drugs,
    p.evidenceLevel AS evidenceLevel,
    p.genotype AS genotype,
    p.genotypeAnnotationText AS genotypeAnnotationText,
    p.genotypeId AS genotypeId,
    p.haplotypeFromSourceId AS haplotypeFromSourceId,
    p.haplotypeId AS haplotypeId,
    p.literature AS literature,
    p.pgxCategory AS pgxCategory,
    p.phenotypeFromSourceId AS phenotypeFromSourceId,
    p.phenotypeText AS phenotypeText,
    p.variantAnnotation AS variantAnnotation,
    p.studyId AS studyId,
    p.targetFromSourceId AS targetFromSourceId,
    p.variantFunctionalConsequenceId AS variantFunctionalConsequenceId,
    p.variantRsId AS variantRsId,
    p.variantId AS variantId,
    p.isDirectTarget AS isDirectTarget,
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
    arrayMap(
      d -> toString(assumeNotNull(d.drugId)),
      arrayFilter(d -> isNotNull(d.drugId), p.drugs)
    ) AS drugIds,
    arrayMap(d -> toString(d.drugFromSource), p.drugs) AS drugFromSources
  FROM
    pharmacogenomics AS p
)
