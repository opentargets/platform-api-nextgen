Q
WITH
  1.6449240668982423 AS max_hs_score,
  arrayReverseSort(
    x -> x .2,
    groupArray(
      tuple(
        divide(score_datasource, max_hs_score),
        divide(
          multiply(score_datasource, datasource_weight),
          max_hs_score
        ),
        datasourceId,
        datatypeId
      )
    )
  ) AS scores_vector,
  arrayMap(
    (i, j) -> (i .1, (i .2) / pow(j, 2), i .3, i .4),
    scores_vector,
    arrayEnumerate(scores_vector)
  ) AS datasource_scores,
  arrayMap(x -> (x .3, x .1), datasource_scores) AS score_datasources,
  arrayMap(x -> (x .4, x .1), datasource_scores) AS score_dt,
  groupUniqArray(datatypeId) AS datatypes_v,
  arrayMap(
    x -> (
      x,
      arrayReverseSort(
        arrayMap(b -> b .2, arrayFilter(a -> a .1 = x, score_dt))
      )
    ),
    datatypes_v
  ) AS mapped_dts,
  arrayMap(
    x -> (
      x .1,
      arraySum(
        (i, j) -> i / pow(j, 2),
        x .2,
        arrayEnumerate(x .2)
      ) / arraySum(
        arrayMap(
          (x, y) -> x / pow(y, 2),
          replicate(1.0, x .2),
          arrayEnumerate(x .2)
        )
      )
    ),
    mapped_dts
  ) AS score_datatypes,
  divide(
    arraySum(tupleElement(datasource_scores, 2)),
    max_hs_score
  ) AS score,
  any(noveltyWhereA) AS novelty,
  concat(score_datatypes, score_datasources),
  if(
    notEquals(
      indexOf(
        tupleElement(concat(score_datatypes, score_datasources), 1),
        'score'
      ),
      0
    ),
    tupleElement(
      arrayElement(
        concat(score_datatypes, score_datasources),
        indexOf(
          tupleElement(concat(score_datatypes, score_datasources), 1),
          'score'
        )
      ),
      2
    ),
    0.0
  ) AS score_indexed
SELECT
  B,
  score,
  score_datatypes,
  score_datasources,
  novelty
FROM
  (
    WITH
      arraySum(
        arrayMap(
          (x, y) -> x / pow(y, 2),
          arrayReverseSort(groupArray(rowScore)),
          arrayEnumerate(groupArray(rowScore))
        )
      ) AS score_datasource,
      any(datatypeId) AS datatypeId,
      ifNull(any(weight), 1.0) AS datasource_weight
    SELECT
      B,
      datasource_weight,
      datatypeId,
      datasourceId,
      score_datasource,
      anyIf (noveltyDirect, equals(A, 'ENSG00000105397')) AS noveltyWhereA
    FROM
      platform2606.associations_otf_target l
      LEFT OUTER JOIN (
        WITH
          arrayJoin(
            array(
              tuple('clinical_precedence', 1.0),
              tuple('gwas_credible_sets', 1.0),
              tuple('gene_burden', 1.0),
              tuple('eva', 1.0),
              tuple('genomics_england', 1.0),
              tuple('gene2phenotype', 1.0),
              tuple('uniprot_literature', 1.0),
              tuple('uniprot_variants', 1.0),
              tuple('orphanet', 1.0),
              tuple('clingen', 1.0),
              tuple('cancer_gene_census', 1.0),
              tuple('intogen', 1.0),
              tuple('eva_somatic', 1.0),
              tuple('cancer_biomarkers', 1.0),
              tuple('crispr_screen', 1.0),
              tuple('crispr', 1.0),
              tuple('reactome', 1.0),
              tuple('europepmc', 0.2),
              tuple('expression_atlas', 0.2),
              tuple('impc', 0.2),
              tuple('ot_crispr_validation', 0.5),
              tuple('ot_crispr', 0.5),
              tuple('encore', 0.5)
            )
          ) AS weightPair
        SELECT
          tupleElement(weightPair, 1) AS datasourceId,
          toNullable(tupleElement(weightPair, 2)) AS weight
        ORDER BY
          datasourceId ASC
      ) r USING (datasourceId)
    PREWHERE
      (
        and (
          like (searchB, lower('%%')),
          and (
            and (
              or (
                and (
                  in (A, ('ENSG00000105397')),
                  notIn(datasourceId, ('expression_atlas'))
                ),
                equals(A, 'ENSG00000105397')
              ),
              in (B, ('MONDO_0100096'))
            ),
            equals(isMeasurement, false)
          )
        )
      )
    GROUP BY
      B,
      datasourceId
  )
GROUP BY
  B
ORDER BY
  score DESC
LIMIT
  25 OFFSET 0
