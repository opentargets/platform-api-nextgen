WITH
    1.6449240668982423 AS max_hs_score,
    arrayReverseSort(x -> (x.2), groupArray((score_datasource / max_hs_score, (score_datasource * datasource_weight) / max_hs_score, datasourceId, datatypeId))) AS scores_vector,
    arrayMap((i, j) -> (i.1, (i.2) / pow(j, 2), i.3, i.4), scores_vector, arrayEnumerate(scores_vector)) AS datasource_scores,
    arrayMap(x -> (x.3, x.1), datasource_scores) AS score_datasources,
    arrayMap(x -> (x.4, x.1), datasource_scores) AS score_dt,
    groupUniqArray(datatypeId) AS datatypes_v,
    arrayMap(x -> (x, arrayReverseSort(arrayMap(b -> (b.2), arrayFilter(a -> ((a.1) = x), score_dt)))), datatypes_v) AS mapped_dts,
    arrayMap(x -> (x.1, arraySum((i, j) -> (i / pow(j, 2)), x.2, arrayEnumerate(x.2)) / arraySum(arrayMap((x, y) -> (x / pow(y, 2)), replicate(1., x.2), arrayEnumerate(x.2)))), mapped_dts) AS score_datatypes,
    arraySum(datasource_scores.2) / max_hs_score AS score,
    any(noveltyWhereA) AS novelty,
    concat(score_datatypes, score_datasources),
    if(indexOf(concat(score_datatypes, score_datasources).1, 'score') != 0, (concat(score_datatypes, score_datasources)[indexOf(concat(score_datatypes, score_datasources).1, 'score')]).2, 0.) AS score_indexed
SELECT
    B,
    score,
    score_datatypes,
    score_datasources,
    novelty
FROM
(
    WITH
        arraySum(arrayMap((x, y) -> (x / pow(y, 2)), arrayReverseSort(groupArray(rowScore)), arrayEnumerate(groupArray(rowScore)))) AS score_datasource,
        any(datatypeId) AS datatypeId,
        ifNull(any(weight), 1.) AS datasource_weight
    SELECT
        B,
        datasource_weight,
        datatypeId,
        datasourceId,
        score_datasource,
        anyIf(noveltyDirect, A = 'ENSG00000105397') AS noveltyWhereA
    FROM platform2606.associations_otf_target AS l
    LEFT JOIN
    (
        WITH arrayJoin([('clinical_precedence', 1.), ('gwas_credible_sets', 1.), ('gene_burden', 1.), ('eva', 1.), ('genomics_england', 1.), ('gene2phenotype', 1.), ('uniprot_literature', 1.), ('uniprot_variants', 1.), ('orphanet', 1.), ('clingen', 1.), ('cancer_gene_census', 1.), ('intogen', 1.), ('eva_somatic', 1.), ('cancer_biomarkers', 1.), ('crispr_screen', 1.), ('crispr', 1.), ('reactome', 1.), ('europepmc', 0.2), ('expression_atlas', 0.2), ('impc', 0.2), ('ot_crispr_validation', 0.5), ('ot_crispr', 0.5), ('encore', 0.5)]) AS weightPair
        SELECT
            weightPair.1 AS datasourceId,
            toNullable(weightPair.2) AS weight
        ORDER BY datasourceId ASC
    ) AS r USING (datasourceId)
    PREWHERE (searchB LIKE lower('%%')) AND (((((A IN ('ENSG00000105397')) AND (datasourceId NOT IN ('expression_atlas'))) OR (A = 'ENSG00000105397')) AND (B IN ('MONDO_0100096'))) AND (isMeasurement = false))
    GROUP BY
        B,
        datasourceId
)
GROUP BY B
ORDER BY score DESC
LIMIT 0, 25
