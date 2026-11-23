//! Runs every `tests/live/<product>/*.graphql` against a live ClickHouse database and asserts
//! that none returns a GraphQL error.
//!
//! ```sh
//! LIVE_CLICKHOUSE_URL=http://localhost:8123 \
//!     cargo test --features product-platform --test live_clickhouse -- --ignored
//! ```
//!
//! A query file may start with `# variables: {...}`. With `LIVE_FIXTURES=1`, each
//! `tests/live/fixtures/<table>.jsonl` replaces the rows of `<table>` first: scratch databases
//! only.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use async_graphql::{
    Context, EmptyMutation, EmptySubscription, MergedObject, Object, PathSegment, Request, Schema,
    Variables,
};
use axum::body::Bytes;
use figment::{
    Figment,
    providers::{Format, Toml},
};
#[cfg(feature = "product-ppp")]
use platform_api::entity::association_timeseries_ppp::{
    AssociationTimeseries, AssociationTimeseriesArguments, TimeseriesKey,
    load_association_timeseries,
};
use platform_api::{
    AppState,
    config::Config,
    datasource::{clickhouse::ClickHouse, opensearch::OpenSearch},
    entity::{
        association::{
            Association, AssociationArguments, AssociationSort, DISEASE_INDIRECT_WEIGHT_DEFAULT,
            DatasourcePolicyOverride, TARGET_INDIRECT_WEIGHT_DEFAULT, load_associations,
        },
        disease::Disease,
        evidence::{Evidence, EvidenceKey, load_evidences},
        evidence_source::EvidenceSourceQuery,
        target::Target,
    },
    product::{CurrentProduct, Product},
    query::paginate::{Page, Paged},
    view::evidence::{EvidenceScope, EvidenceViews},
};
use serde_json::json;
use tokio::sync::OnceCell;

const LIVE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live");

fn config(clickhouse_url: &str) -> Config {
    let release: Vec<&str> = env!("CARGO_PKG_VERSION").split('.').take(2).collect();
    let toml = format!(
        "data_release = \"{}\"\nclickhouse_url = {clickhouse_url:?}\n\
         opensearch_url = \"http://localhost:9200\"\nmax_depth = 64\n\
         max_complexity = 1_000_000_000\n",
        release.join(".")
    );
    Figment::from(Toml::string(&toml))
        .extract()
        .expect("live test config")
}

/// Replaces the rows of `table` with the `JSONEachRow` lines of `jsonl_path`.
///
/// # Panics
/// Panics if the file cannot be read or ClickHouse rejects the rows.
pub async fn insert_fixture(ch: &ClickHouse, table: &str, jsonl_path: &Path) {
    let rows = fs::read(jsonl_path).unwrap_or_else(|e| panic!("{}: {e}", jsonl_path.display()));
    ch.client()
        .query(&format!("TRUNCATE TABLE `{table}`"))
        .execute()
        .await
        .unwrap_or_else(|e| panic!("truncate {table}: {e}"));
    insert_rows(ch, table, rows).await;
}

/// Appends `JSONEachRow` lines to `table`.
///
/// # Panics
/// Panics if ClickHouse rejects the rows.
async fn insert_rows(ch: &ClickHouse, table: &str, rows: impl Into<Bytes>) {
    let mut insert = ch
        .client()
        .insert_formatted_with(format!("INSERT INTO `{table}` FORMAT JSONEachRow"));
    insert
        .send(rows.into())
        .await
        .unwrap_or_else(|e| panic!("insert {table}: {e}"));
    insert
        .end()
        .await
        .unwrap_or_else(|e| panic!("insert {table}: {e}"));
}

fn with_fixtures() -> bool { std::env::var("LIVE_FIXTURES").is_ok_and(|v| v == "1") }

/// Inserts every fixture once per test binary, so tests running in parallel see the same rows.
async fn load_fixtures(ch: &ClickHouse) {
    static LOADED: OnceCell<()> = OnceCell::const_new();
    LOADED
        .get_or_init(|| async {
            for path in files(&Path::new(LIVE).join("fixtures"), "jsonl") {
                let table = path.file_stem().and_then(|s| s.to_str()).expect("table");
                insert_fixture(ch, table, &path).await;
            }
        })
        .await;
}

fn files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == extension))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// The query and its variables, read from a `# variables: {...}` first line.
fn request(path: &Path) -> Request {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let variables = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# variables:"))
        .map(|json| {
            serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("{}: bad variables line: {e}", path.display()))
        });
    let request = Request::new(text.clone());
    match variables {
        Some(json) => request.variables(Variables::from_json(json)),
        None => request,
    }
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL"]
async fn live_queries_return_no_errors() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    let config = config(&url);
    let ch = ClickHouse::new(&config);

    if with_fixtures() {
        load_fixtures(&ch).await;
    }

    let state = AppState::new(config);
    let product = <CurrentProduct as Product>::NAME.to_string();
    let queries = files(&Path::new(LIVE).join(&product), "graphql");
    assert!(!queries.is_empty(), "no queries in tests/live/{product}");

    let mut errors = Vec::new();
    for path in &queries {
        let response = state.api.execute(request(path)).await;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        for error in &response.errors {
            let at: Vec<String> = error
                .path
                .iter()
                .map(|s| match s {
                    PathSegment::Field(f) => f.clone(),
                    PathSegment::Index(i) => i.to_string(),
                })
                .collect();
            errors.push(format!("{name}: {}: {}", at.join("."), error.message));
        }
    }
    errors.sort();
    assert!(
        errors.is_empty(),
        "{} GraphQL errors in {} query files:\n{}",
        errors.len(),
        queries.len(),
        errors.join("\n")
    );
}

// ---- evidence fixtures decode to the inserted values ----

const FIXTURE_DISEASE: &str = "EFO_9999999";
const FIXTURE_TARGET: &str = "ENSG99999999999";

/// Evidence and the evidence views for the fixture disease/target pair, without the
/// `Disease`/`Target` roots.
struct EvidenceRoot;

#[Object]
impl EvidenceRoot {
    async fn evidences(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Evidence>> {
        load_evidences(
            ctx,
            EvidenceKey {
                efo_ids: vec![FIXTURE_DISEASE.to_string()],
                ensembl_ids: vec![FIXTURE_TARGET.to_string()],
                datasource_ids: Vec::new(),
            },
        )
        .await
    }

    async fn views(&self) -> EvidenceViews {
        EvidenceViews::new(EvidenceScope::new(
            vec![FIXTURE_DISEASE.to_string()],
            vec![FIXTURE_TARGET.to_string()],
            false,
        ))
    }
}

const EVIDENCE_FIELDS: &str = "id datasourceId datatypeId score clinicalStage directionOnTarget
    directionOnTrait diseaseFromSourceMappedId targetFromSourceId trialStopReasonCategories
    biomarkers { geneExpression { name } geneticVariation { id name } }
    diseaseCellLines { id name tissue tissueId }
    mutatedSamples { numberMutatedSamples numberSamplesTested numberSamplesWithMutationType }
    pathways { id name }
    textMiningSentences { dEnd dStart section tEnd tStart text }
    urls { niceName url }
    diseaseModelAssociatedHumanPhenotypes { id label }
    publicationYear studyCases log2FoldChangePercentileRank";

#[cfg(feature = "product-ppp")]
const PPP_EVIDENCE_FIELDS: &str = "assays { description isHit shortName } assessments
    biomarkerList { name description } cellLineBackground geneticInteractionScore
    geneticInteractionType interactingTargetFromSourceId interactingTargetRole
    phenotypicConsequenceLogFoldChange phenotypicConsequencePValue primaryProjectHit
    primaryProjectId projectDescription targetRole";
#[cfg(not(feature = "product-ppp"))]
const PPP_EVIDENCE_FIELDS: &str = "";

const VIEWS_QUERY: &str = "views {
    clinicalPrecedence { rows { id directionOnTarget directionOnTrait clinicalStage drugName } }
    gwasCredibleSets { rows { id credibleSetId fineMappingMethod leadChromosome } }
    geneBurden { rows { id targetFromSourceId urls directionOnTarget directionOnTrait } }
    eva { rows { id directionOnTarget directionOnTrait variantChromosome } }
    gene2Phenotype { rows { id targetFromSourceId directionOnTarget directionOnTrait } }
    uniprotLiterature { rows { id targetFromSourceId } }
    uniprotVariants { rows { id targetFromSourceId } }
    genomicsEnglandPanelApp { rows { id confidence } }
    impc { rows { id directionOnTarget directionOnTrait
        diseaseModelAssociatedModelPhenotypes { id label } } }
    clinicalLof: clinicalPrecedence(search: {text: \"lof\"}) { count }
    clinicalPhase3: clinicalPrecedence(search: {text: \"phase_3\"}) { count }
    gwasPics: gwasCredibleSets(search: {text: \"pics\"}) { count }
}";

/// `rows` keyed by `id`.
fn by_id(rows: &serde_json::Value) -> HashMap<String, serde_json::Value> {
    rows.as_array()
        .expect("rows")
        .iter()
        .map(|r| (r["id"].as_str().expect("id").to_string(), r.clone()))
        .collect()
}

/// Asserts each field of `expected` equals the same field of `row`.
fn assert_fields(what: &str, row: &serde_json::Value, expected: &serde_json::Value) {
    for (field, value) in expected.as_object().expect("object") {
        assert_eq!(&row[field], value, "{what}: {field}");
    }
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL and LIVE_FIXTURES=1"]
async fn evidence_fixtures_decode_to_inserted_values() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    assert!(with_fixtures(), "set LIVE_FIXTURES=1");
    let ch = ClickHouse::new(&config(&url));
    load_fixtures(&ch).await;

    let schema = Schema::new(EvidenceRoot, EmptyMutation, EmptySubscription);
    let query =
        format!("{{ evidences {{ {EVIDENCE_FIELDS} {PPP_EVIDENCE_FIELDS} }} {VIEWS_QUERY} }}");
    let response = schema
        .execute(CurrentProduct::prepare_request(Request::new(query), &ch))
        .await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json().expect("json");

    let evidences = by_id(&data["evidences"]);
    assert_eq!(evidences.len(), 18);
    let full = &evidences["live-test-evidence-full-clinical_precedence"];
    assert_fields(
        "evidences",
        full,
        &json!({
            "datasourceId": "clinical_precedence",
            "datatypeId": "clinical",
            "clinicalStage": "PHASE_3",
            "directionOnTarget": "LoF",
            "directionOnTrait": "risk",
            "diseaseFromSourceMappedId": "EFO_9999999",
            "targetFromSourceId": "P15056",
            "trialStopReasonCategories": ["Insufficient_Enrollment", "Negative"],
            "biomarkers": {
                "geneExpression": [{"name": "BRAF overexpression"}],
                "geneticVariation": [{"id": "7_140753336_A_T", "name": "BRAF V600E"}],
            },
            "diseaseCellLines": [
                {"id": "SIDM00001", "name": "A375", "tissue": "skin", "tissueId": "UBERON_0002097"},
            ],
            "mutatedSamples": [{
                "numberMutatedSamples": 12,
                "numberSamplesTested": 100,
                "numberSamplesWithMutationType": 8,
            }],
            "pathways": [{"id": "R-HSA-5673001", "name": "RAF/MAP kinase cascade"}],
            "textMiningSentences": [{
                "dEnd": 20,
                "dStart": 12,
                "section": "abstract",
                "tEnd": 4,
                "tStart": 0,
                "text": "BRAF in melanoma.",
            }],
            "urls": [{
                "niceName": "ClinicalTrials.gov",
                "url": "https://clinicaltrials.gov/study/NCT01234567",
            }],
            "diseaseModelAssociatedHumanPhenotypes": [{"id": "HP_0002861", "label": "Melanoma"}],
            "publicationYear": 2002,
            "studyCases": 1500,
            "log2FoldChangePercentileRank": 5,
        }),
    );
    #[cfg(feature = "product-ppp")]
    assert_fields(
        "ppp evidences",
        full,
        &json!({
            "assays": [{
                "description": "Cell viability after knockout",
                "isHit": true,
                "shortName": "viability",
            }],
            "assessments": ["primary screen"],
            "biomarkerList": [{"name": "BRAF V600E", "description": "BRAF V600E somatic mutation"}],
            "cellLineBackground": "HAP1",
            "geneticInteractionScore": -0.4,
            "targetRole": "query",
        }),
    );
    assert_fields(
        "null evidences",
        &evidences["live-test-evidence-null-gene2phenotype"],
        &json!({
            "clinicalStage": null,
            "directionOnTarget": null,
            "directionOnTrait": null,
            "trialStopReasonCategories": [],
            "biomarkers": {
                "geneExpression": [],
                "geneticVariation": [{"id": null, "name": "BRAF amplification"}],
            },
            "mutatedSamples": [{
                "numberMutatedSamples": 1,
                "numberSamplesTested": 2,
                "numberSamplesWithMutationType": null,
            }],
            "urls": [{"niceName": null, "url": "https://www.ebi.ac.uk/gene2phenotype"}],
            "publicationYear": null,
        }),
    );

    let views = &data["views"];
    let full_and_null = |view: &str, ds: &str| {
        let rows = by_id(&views[view]["rows"]);
        assert_eq!(rows.len(), 2, "{view}: {rows:?}");
        (
            rows[&format!("live-test-evidence-full-{ds}")].clone(),
            rows[&format!("live-test-evidence-null-{ds}")].clone(),
        )
    };
    for (view, ds) in [
        ("geneBurden", "gene_burden"),
        ("eva", "eva"),
        ("gene2Phenotype", "gene2phenotype"),
        ("impc", "impc"),
    ] {
        let (full, null) = full_and_null(view, ds);
        assert_fields(
            view,
            &full,
            &json!({"directionOnTarget": "LoF", "directionOnTrait": "risk"}),
        );
        assert_fields(
            view,
            &null,
            &json!({"directionOnTarget": null, "directionOnTrait": null}),
        );
    }
    for (view, ds) in [
        ("geneBurden", "gene_burden"),
        ("gene2Phenotype", "gene2phenotype"),
        ("uniprotLiterature", "uniprot_literature"),
        ("uniprotVariants", "uniprot_variants"),
    ] {
        let (full, null) = full_and_null(view, ds);
        assert_fields(view, &full, &json!({"targetFromSourceId": "P15056"}));
        assert_fields(view, &null, &json!({"targetFromSourceId": "BRAF"}));
    }
    let (full, null) = full_and_null("geneBurden", "gene_burden");
    assert_fields(
        "geneBurden",
        &full,
        &json!({"urls": ["https://clinicaltrials.gov/study/NCT01234567"]}),
    );
    assert_fields(
        "geneBurden",
        &null,
        &json!({"urls": ["https://www.ebi.ac.uk/gene2phenotype"]}),
    );
    let (full, _) = full_and_null("impc", "impc");
    assert_fields(
        "impc",
        &full,
        &json!({"diseaseModelAssociatedModelPhenotypes": [
            {"id": "MP_0002020", "label": "increased tumor incidence"},
        ]}),
    );
    let (full, null) = full_and_null("genomicsEnglandPanelApp", "genomics_england");
    assert_fields(
        "genomicsEnglandPanelApp",
        &full,
        &json!({"confidence": "green"}),
    );
    assert_fields(
        "genomicsEnglandPanelApp",
        &null,
        &json!({"confidence": null}),
    );
    let (full, _) = full_and_null("gwasCredibleSets", "gwas_credible_sets");
    assert_fields(
        "gwasCredibleSets",
        &full,
        &json!({"credibleSetId": "live-test-credible-set"}),
    );

    // The clinical precedence NULL row is `live-test-evidence`.
    let clinical = by_id(&views["clinicalPrecedence"]["rows"]);
    assert_eq!(clinical.len(), 2, "{clinical:?}");
    assert_fields(
        "clinicalPrecedence",
        &clinical["live-test-evidence-full-clinical_precedence"],
        &json!({"clinicalStage": "PHASE_3", "directionOnTarget": "LoF", "directionOnTrait": "risk"}),
    );
    assert_fields(
        "clinicalPrecedence",
        &clinical["live-test-evidence"],
        &json!({"clinicalStage": null, "directionOnTarget": null, "directionOnTrait": null}),
    );
    // Enum columns match by name; unmatched credible sets join as the first enum value.
    assert_eq!(views["clinicalLof"]["count"], 1);
    assert_eq!(views["clinicalPhase3"]["count"], 1);
    assert_eq!(views["gwasPics"]["count"], 2);
}

// ---- association fixtures decode to the inserted values ----

/// Associations for a fixture anchor, without the `Disease`/`Target` roots.
struct AssociationRoot;

fn association_args(
    indirect_weight: f64,
    overrides: Option<Vec<DatasourcePolicyOverride>>,
    sort: Option<AssociationSort>,
) -> AssociationArguments {
    AssociationArguments {
        bs: Vec::new(),
        b_filter: None,
        facet_filters: Vec::new(),
        include_measurements: None,
        indirect_weight,
        datasource_policy_overrides: overrides.unwrap_or_default(),
        sort: sort.unwrap_or_default(),
        page: Page::default(),
    }
}

#[Object]
impl AssociationRoot {
    async fn target_associations(
        &self,
        ctx: &Context<'_>,
        anchor: Option<String>,
        overrides: Option<Vec<DatasourcePolicyOverride>>,
        sort: Option<AssociationSort>,
    ) -> async_graphql::Result<Paged<Association<Disease>>> {
        let anchor = anchor.unwrap_or_else(|| FIXTURE_TARGET.to_string());
        let args = association_args(TARGET_INDIRECT_WEIGHT_DEFAULT, overrides, sort);
        load_associations::<Target>(ctx, &anchor, &args).await
    }

    async fn disease_associations(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Paged<Association<Target>>> {
        let args = association_args(DISEASE_INDIRECT_WEIGHT_DEFAULT, None, None);
        load_associations::<Disease>(ctx, FIXTURE_DISEASE, &args).await
    }

    #[cfg(feature = "product-ppp")]
    async fn time_series(
        &self,
        ctx: &Context<'_>,
        is_direct: bool,
    ) -> async_graphql::Result<Paged<AssociationTimeseries>> {
        load_association_timeseries(
            ctx,
            TimeseriesKey {
                disease_id: FIXTURE_DISEASE.to_string(),
                target_id: FIXTURE_TARGET.to_string(),
            },
            &AssociationTimeseriesArguments {
                is_direct,
                aggregation_types: None,
                start_year: None,
                end_year: None,
                page: Page::default(),
            },
        )
        .await
    }
}

#[derive(MergedObject)]
struct AssociationQuery(AssociationRoot, EvidenceSourceQuery);

/// The `datasourceScores` ids of each row.
fn datasource_ids(page: &serde_json::Value) -> Vec<Vec<String>> {
    page["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            row["datasourceScores"]
                .as_array()
                .expect("datasourceScores")
                .iter()
                .map(|s| s["id"].as_str().expect("id").to_string())
                .collect()
        })
        .collect()
}

fn assert_close(what: &str, value: &serde_json::Value, expected: f64) {
    let value = value.as_f64().unwrap_or_else(|| panic!("{what}: {value}"));
    assert!(
        (value - expected).abs() < 1e-9,
        "{what}: {value} != {expected}"
    );
}

const ASSOCIATIONS_QUERY: &str = "
    associationDatasources { datasource datatype }
    targetAssociations { count rows { score datasourceScores { id score } } }
    required: targetAssociations(overrides: [
        {id: impc, policy: {weight: 1.0, required: true}}
    ]) { count rows { datasourceScores { id } } }
    byImpc: targetAssociations(sort: {key: \"impc\", direction: DESCENDING}) {
        count rows { datasourceScores { id } }
    }
    diseaseAssociations { count rows { datasourceScores { id } } }";

#[cfg(feature = "product-ppp")]
const PPP_ASSOCIATIONS_QUERY: &str = "
    novelty: diseaseAssociations { rows { novelty datasourceScores { id } } }
    pppOnly: targetAssociations(anchor: \"ENSG99999999990\") { rows { datasourceScores { id } } }
    direct: timeSeries(isDirect: true) { rows {
        aggregationType aggregationValue year associationScore novelty yearlyEvidenceCount
    } }
    indirect: timeSeries(isDirect: false) { rows { aggregationType aggregationValue } }";
#[cfg(not(feature = "product-ppp"))]
const PPP_ASSOCIATIONS_QUERY: &str = "";

/// PPP-only rows: the PPP datasources, and the time series (a PPP-only table).
#[cfg(feature = "product-ppp")]
async fn insert_ppp_association_rows(ch: &ClickHouse) {
    let row = |id: &str, ds: &str, score: f64| {
        format!(
            "{{\"rowId\":\"{id}\",\"A\":\"ENSG99999999990\",\"B\":\"EFO_9999999\",\
             \"datasourceId\":\"{ds}\",\"datatypeId\":\"ot_partner\",\"rowScore\":{score},\
             \"isMeasurement\":0,\"indirect\":[]}}\n"
        )
    };
    let rows = [
        row("live-test-ppp-1", "encore", 0.5),
        row("live-test-ppp-2", "ot_crispr", 0.2),
        row("live-test-ppp-3", "ot_crispr_validation", 0.9),
    ]
    .concat();
    insert_rows(ch, "associations_otf_target", rows).await;

    ch.client()
        .query("TRUNCATE TABLE association_time_series")
        .execute()
        .await
        .expect("truncate association_time_series");
    let series = [
        r#"{"diseaseId":"EFO_9999999","targetId":"ENSG99999999999","aggregationType":"overall","aggregationValue":"None","year":2020,"associationScore":0.5,"novelty":0.125,"yearlyEvidenceCount":3,"isDirect":true}"#,
        r#"{"diseaseId":"EFO_9999999","targetId":"ENSG99999999999","aggregationType":"datasourceId","aggregationValue":"eva","year":null,"associationScore":0.25,"novelty":null,"yearlyEvidenceCount":null,"isDirect":true}"#,
        r#"{"diseaseId":"EFO_9999999","targetId":"ENSG99999999999","aggregationType":"datasourceId","aggregationValue":"ot_crispr","year":2021,"associationScore":0.75,"novelty":null,"yearlyEvidenceCount":1,"isDirect":false}"#,
    ]
    .join("\n");
    insert_rows(ch, "association_time_series", series).await;
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL and LIVE_FIXTURES=1"]
async fn association_fixtures_decode_to_inserted_values() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    assert!(with_fixtures(), "set LIVE_FIXTURES=1");
    let config = config(&url);
    let ch = ClickHouse::new(&config);
    load_fixtures(&ch).await;
    #[cfg(feature = "product-ppp")]
    insert_ppp_association_rows(&ch).await;

    let schema = Schema::build(
        AssociationQuery(AssociationRoot, EvidenceSourceQuery),
        EmptyMutation,
        EmptySubscription,
    )
    .data(ch.clone())
    .data(OpenSearch::new(&config))
    .finish();
    let query = format!("{{ {ASSOCIATIONS_QUERY} {PPP_ASSOCIATIONS_QUERY} }}");
    let response = schema
        .execute(CurrentProduct::prepare_request(Request::new(query), &ch))
        .await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json().expect("json");

    let mut sources: Vec<(String, String)> = data["associationDatasources"]
        .as_array()
        .expect("associationDatasources")
        .iter()
        .map(|s| {
            (
                s["datasource"].as_str().expect("datasource").to_string(),
                s["datatype"].as_str().expect("datatype").to_string(),
            )
        })
        .collect();
    sources.sort();
    assert_eq!(
        sources,
        [
            ("clinical_precedence", "known_drug"),
            ("eva", "genetic_association"),
            ("impc", "animal_model"),
        ]
        .map(|(d, t)| (d.to_string(), t.to_string()))
    );

    // EFO_9999999 then EFO_9999998; datasources by weighted score.
    let max_hs = std::f64::consts::PI.powi(2) / 6.0;
    let target = &data["targetAssociations"];
    assert_eq!(target["count"], 2);
    assert_eq!(
        datasource_ids(target),
        [
            vec!["eva", "clinical_precedence", "expression_atlas"],
            vec!["impc"],
        ]
    );
    let scores = &target["rows"][0]["datasourceScores"];
    assert_close("eva", &scores[0]["score"], 0.9 / max_hs);
    assert_close("clinical_precedence", &scores[1]["score"], 0.5 / max_hs);
    assert_close("expression_atlas", &scores[2]["score"], 0.6 / max_hs);
    let overall = (0.9 + 0.5 / 4.0 + 0.6 * 0.2 / 9.0) / max_hs / max_hs;
    assert_close("score", &target["rows"][0]["score"], overall);

    assert_eq!(data["required"]["count"], 1);
    assert_eq!(datasource_ids(&data["required"]), [vec!["impc"]]);
    assert_eq!(
        datasource_ids(&data["byImpc"]),
        [
            vec!["impc"],
            vec!["eva", "clinical_precedence", "expression_atlas"],
        ]
    );
    assert_eq!(data["diseaseAssociations"]["count"], 2);
    assert_eq!(
        datasource_ids(&data["diseaseAssociations"]),
        [vec!["eva", "clinical_precedence"], vec!["impc"]]
    );

    #[cfg(feature = "product-ppp")]
    {
        let novelty = &data["novelty"]["rows"];
        assert_eq!(novelty[0]["novelty"], 0.25);
        assert_eq!(novelty[1]["novelty"], serde_json::Value::Null);
        assert_eq!(
            datasource_ids(&data["pppOnly"]),
            [vec!["ot_crispr_validation", "encore", "ot_crispr"]]
        );
        let mut direct = data["direct"]["rows"].as_array().expect("rows").clone();
        direct.sort_by_key(|r| r["aggregationType"].to_string());
        assert_eq!(
            serde_json::Value::Array(direct),
            json!([
                {
                    "aggregationType": "datasourceId",
                    "aggregationValue": "eva",
                    "year": null,
                    "associationScore": 0.25,
                    "novelty": null,
                    "yearlyEvidenceCount": null,
                },
                {
                    "aggregationType": "overall",
                    "aggregationValue": null,
                    "year": 2020,
                    "associationScore": 0.5,
                    "novelty": 0.125,
                    "yearlyEvidenceCount": 3,
                },
            ])
        );
        assert_eq!(
            data["indirect"]["rows"],
            json!([{"aggregationType": "datasourceId", "aggregationValue": "ot_crispr"}])
        );
    }
}

// ---- target fixtures decode to the inserted values ----

const TARGET_FIELDS: &str = "fragment T on Target {
    id approvedSymbol approvedName biotype alternativeGenes functionDescriptions transcriptIds
    canonicalTranscript { id chromosome start end strand }
    genomicLocation { chromosome start end strand }
    chemicalProbes { id control drugId drugFromSourceId mechanismOfAction isHighQuality origin
        probesDrugsScore scoreInCells scoreInOrganisms targetFromSourceId urls { niceName url } }
    dbXrefs { id source }
    geneticConstraint { constraintType exp obs oe oeLower oeUpper score upperBin upperBin6
        upperRank }
    geneOntology { aspect evidence geneProduct source }
    hallmarks { cancerHallmarks { description impact label pmid }
        attributes { name description pmid } }
    homologues { homologyType queryPercentageIdentity speciesId speciesName targetGeneId
        targetGeneSymbol targetPercentageIdentity isHighConfidence }
    pathways { pathway pathwayId topLevelTerm }
    proteinIds { id source }
    safetyLiabilities { biosamples { tissueLabel tissueId cellLabel cellFormat cellId }
        datasource effects { direction dosing } event eventId literature url
        studies { name description type } }
    subcellularLocations { location source termSL labelSL targetModifier }
    synonyms { label source } symbolSynonyms { label source } nameSynonyms { label source }
    obsoleteSymbols { label source } obsoleteNames { label source }
    targetClass { id label level }
    tractability { label modality value }
    transcripts { transcriptId biotype isEnsemblCanonical uniprotId isUniprotReviewed
        translationId alphafoldId uniprotIsoformId }
    isEssential
    depMapEssentiality { count rows { tissueId tissueName screens { cellLineName depmapId
        diseaseCellLineId diseaseFromSource expression geneEffect mutation } } }
}";

/// `tests/live/fixtures/targets.jsonl` and `target_essentiality.jsonl` as the API serves them.
#[allow(clippy::too_many_lines)]
fn expected_target() -> serde_json::Value {
    let label = |label: &str, source: &str| json!([{"label": label, "source": source}]);
    let chemical_probes = json!([
        {
            "id": "PLX4720", "control": "PLX-control", "drugId": "CHEMBL1230020",
            "drugFromSourceId": "PD-PLX4720", "mechanismOfAction": ["inhibitor"],
            "isHighQuality": true, "origin": ["calculated"], "probesDrugsScore": 0.75,
            "scoreInCells": 0.5, "scoreInOrganisms": 0.25, "targetFromSourceId": "P15056",
            "urls": [
                {"niceName": "Probes&Drugs",
                 "url": "https://www.probes-drugs.org/compound/PD000001"},
                {"niceName": "SGC", "url": null},
            ],
        },
        {
            "id": "GDC-0879", "control": null, "drugId": null,
            "drugFromSourceId": "PD-GDC0879", "mechanismOfAction": [],
            "isHighQuality": false, "origin": [], "probesDrugsScore": 0.125,
            "scoreInCells": null, "scoreInOrganisms": null, "targetFromSourceId": "BRAF",
            "urls": [],
        },
    ]);
    let constraint = json!([
        {
            "constraintType": "mis", "exp": 2.5, "obs": 3, "oe": 0.375, "oeLower": 0.25,
            "oeUpper": 0.5, "score": 0.125, "upperBin": 8, "upperBin6": 4, "upperRank": 7,
        },
        {
            "constraintType": "syn", "exp": null, "obs": null, "oe": null, "oeLower": null,
            "oeUpper": null, "score": null, "upperBin": null, "upperBin6": null,
            "upperRank": null,
        },
    ]);
    let go = json!([
        {"aspect": "F", "evidence": "IDA", "geneProduct": "P15056",
         "source": "GO_REF:0000024"},
        {"aspect": "C", "evidence": "IEA", "geneProduct": "P15057",
         "source": "PMID:12068308"},
    ]);
    let hallmarks = json!({
        "cancerHallmarks": [
            {"description": "hallmark description", "impact": "promotes",
             "label": "hallmark label", "pmid": 22222},
            {"description": "hallmark description 2", "impact": null,
             "label": "hallmark label 2", "pmid": 22223},
        ],
        "attributes": [
            {"name": "attribute name", "description": "attribute description",
             "pmid": 11111},
            {"name": "attribute name 2", "description": "attribute description 2",
             "pmid": null},
        ],
    });
    let homologues = json!([
        {
            "homologyType": "ortholog_one2many", "queryPercentageIdentity": 96.5,
            "speciesId": "10090", "speciesName": "Mouse",
            "targetGeneId": "ENSMUSG00000002413", "targetGeneSymbol": "Braf",
            "targetPercentageIdentity": 97.25, "isHighConfidence": true,
        },
        {
            "homologyType": "within_species_paralog", "queryPercentageIdentity": 80.5,
            "speciesId": "10116", "speciesName": "Rat",
            "targetGeneId": "ENSRNOG00000010957", "targetGeneSymbol": "Braf2",
            "targetPercentageIdentity": 81.25, "isHighConfidence": null,
        },
    ]);
    let safety_liabilities = json!([
        {
            "biosamples": [
                {"tissueLabel": "heart", "tissueId": "UBERON_0000948",
                 "cellLabel": "cardiomyocyte", "cellFormat": "cell line",
                 "cellId": "CL_0000746"},
                {"tissueLabel": null, "tissueId": null, "cellLabel": null,
                 "cellFormat": null, "cellId": null},
            ],
            "datasource": "ToxCast",
            "effects": [
                {"direction": "Activation/Increase/Upregulation", "dosing": "acute"},
                {"direction": "Inhibition/Decrease/Downregulation", "dosing": null},
            ],
            "event": "cardiac arrhythmia", "eventId": "EFO_0004269",
            "literature": "12345678", "url": "https://example.org/safety",
            "studies": [
                {"name": "study name", "description": "study description",
                 "type": "cell_based"},
                {"name": null, "description": null, "type": null},
            ],
        },
        {
            "biosamples": [], "datasource": "AOP-Wiki", "effects": [],
            "event": "hepatotoxicity", "eventId": null, "literature": null, "url": null,
            "studies": [],
        },
    ]);
    let transcripts = json!([
        {
            "transcriptId": "ENST99999999991", "biotype": "protein_coding",
            "isEnsemblCanonical": true, "uniprotId": "P15056", "isUniprotReviewed": true,
            "translationId": "ENSP99999999991", "alphafoldId": "AF-P15056-F1",
            "uniprotIsoformId": "P15056-1",
        },
        {
            "transcriptId": "ENST99999999992", "biotype": "nonsense_mediated_decay",
            "isEnsemblCanonical": false, "uniprotId": null, "isUniprotReviewed": null,
            "translationId": null, "alphafoldId": null, "uniprotIsoformId": null,
        },
    ]);
    let essentiality = json!({"count": 2, "rows": [
        {
            "tissueId": "UBERON_0002097", "tissueName": "skin",
            "screens": [
                {"cellLineName": "A375", "depmapId": "ACH-000001",
                 "diseaseCellLineId": "SIDM00001", "diseaseFromSource": "Melanoma",
                 "expression": 4.25, "geneEffect": -1.5, "mutation": "BRAF V600E"},
                {"cellLineName": "SK-MEL-28", "depmapId": "ACH-000002",
                 "diseaseCellLineId": null, "diseaseFromSource": "Skin Melanoma",
                 "expression": null, "geneEffect": -0.5, "mutation": null},
            ],
        },
        {"tissueId": null, "tissueName": "unspecified tissue", "screens": []},
    ]});
    json!({
        "id": FIXTURE_TARGET,
        "approvedSymbol": "LTGENE1",
        "approvedName": "live test gene 1",
        "biotype": "protein_coding",
        "alternativeGenes": ["ENSG99999999990"],
        "functionDescriptions": ["function description"],
        "transcriptIds": ["ENST99999999991", "ENST99999999992"],
        "canonicalTranscript": {
            "id": "ENST99999999991", "chromosome": "chr7", "start": 140_000_020,
            "end": 140_000_080, "strand": "POSITIVE",
        },
        "genomicLocation": {
            "chromosome": "chr7", "start": 140_000_010, "end": 140_000_090, "strand": "NEGATIVE",
        },
        "chemicalProbes": chemical_probes,
        "dbXrefs": [{"id": "HGNC:1097", "source": "HGNC"}],
        "geneticConstraint": constraint,
        "geneOntology": go,
        "hallmarks": hallmarks,
        "homologues": homologues,
        "pathways": [{
            "pathway": "RAF/MAP kinase cascade", "pathwayId": "R-HSA-5673001",
            "topLevelTerm": "Signal Transduction",
        }],
        "proteinIds": [{"id": "P15056", "source": "uniprot_swissprot"}],
        "safetyLiabilities": safety_liabilities,
        "subcellularLocations": [
            {"location": "Nucleus", "source": "HPA_main", "termSL": "SL-0191",
             "labelSL": "Nucleus label", "targetModifier": "isoform 2"},
            {"location": "Cytosol", "source": "HPA_additional", "termSL": null,
             "labelSL": null, "targetModifier": null},
        ],
        "synonyms": label("synonym label", "synonym source"),
        "symbolSynonyms": label("symbol synonym label", "symbol synonym source"),
        "nameSynonyms": label("name synonym label", "name synonym source"),
        "obsoleteSymbols": label("obsolete symbol label", "obsolete symbol source"),
        "obsoleteNames": label("obsolete name label", "obsolete name source"),
        "targetClass": [{"id": 5, "label": "Kinase", "level": "l2"}],
        "tractability": [{"label": "UniProt loc high conf", "modality": "AB", "value": true}],
        "transcripts": transcripts,
        "isEssential": true,
        "depMapEssentiality": essentiality,
    })
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL and LIVE_FIXTURES=1"]
async fn target_fixtures_decode_to_inserted_values() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    assert!(with_fixtures(), "set LIVE_FIXTURES=1");
    let config = config(&url);
    load_fixtures(&ClickHouse::new(&config)).await;

    let state = AppState::new(config);
    let query = format!(
        "{{ target(ensemblId: \"{FIXTURE_TARGET}\") {{ ...T }}
            targets(ensemblIds: [\"{FIXTURE_TARGET}\"]) {{ count rows {{ ...T }} }}
            region(chromosome: chr7, start: 140000000, end: 140000200) {{
                targets {{ count rows {{ ...T }} }}
            }}
            outside: region(chromosome: chr7, start: 140000101, end: 140000200) {{
                targets {{ count rows {{ id }} }}
            }}
        }}
        {TARGET_FIELDS}"
    );
    let response = state.api.execute(Request::new(query)).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json().expect("json");

    let expected = expected_target();
    let target = &data["target"];
    for (field, value) in expected.as_object().expect("object") {
        assert_eq!(&target[field], value, "target: {field}");
    }
    assert_eq!(target, &expected);
    assert_eq!(data["targets"], json!({"count": 1, "rows": [expected]}));
    assert_eq!(data["region"]["targets"], data["targets"]);
    assert_eq!(data["outside"]["targets"], json!({"count": 0, "rows": []}));
}

// ---- clinical and drug fixtures decode to the inserted values ----

const CLINICAL_FIELDS: &str = "fragment R on ClinicalReport {
    id source clinicalStage phaseFromSource evidenceType title trialStudyType trialDescription
    trialNumberOfArms trialStartDate trialLiterature { id referenceType } trialOverallStatus
    trialWhyStopped trialPrimaryPurpose trialPhase trialStopReasonCategories
    trialSponsor { agencyClass name } qualityControls diseases { diseaseFromSource }
    drugs { drugFromSource drug { id } } countries year sideEffects { diseaseFromSource }
    trialOfficialTitle url origin provider
}
fragment D on Drug {
    id name synonyms { label source } tradeNames { label source } drugType
    crossReferences { source ids } maximumClinicalStage description molblock
}";

/// `tests/live/fixtures/clinical_report.jsonl` as the API serves it.
fn expected_clinical_reports() -> (serde_json::Value, serde_json::Value) {
    let full = json!({
        "id": "live-test-clinical-report", "source": "live source", "clinicalStage": "PHASE_2_3",
        "phaseFromSource": "Phase 2/Phase 3", "evidenceType": "SAFETY", "title": "report title",
        "trialStudyType": "EXPANDED_ACCESS", "trialDescription": "trial description",
        "trialNumberOfArms": 2, "trialStartDate": "2020-01-15",
        "trialLiterature": [
            {"id": "12345678", "referenceType": "RESULT"},
            {"id": null, "referenceType": "DERIVED"},
        ],
        "trialOverallStatus": "TERMINATED", "trialWhyStopped": "why stopped",
        "trialPrimaryPurpose": "SUPPORTIVE_CARE", "trialPhase": "PHASE2_PHASE3",
        "trialStopReasonCategories": ["Safety_Sideeffects", "Negative"],
        "trialSponsor": {"agencyClass": "INDUSTRY", "name": "sponsor name"},
        "qualityControls": ["quality control"],
        "diseases": [
            {"diseaseFromSource": "disease from source"},
            {"diseaseFromSource": "unmapped disease"},
        ],
        "drugs": [
            {"drugFromSource": "drug from source", "drug": {"id": "CHEMBL9999999"}},
            {"drugFromSource": "unmapped drug", "drug": null},
        ],
        "countries": ["GB", "US"], "year": 2020,
        "sideEffects": [{"diseaseFromSource": "side effect from source"}],
        "trialOfficialTitle": "trial official title", "url": "https://example.org/report",
        "origin": "REGULATORY_AGENCY", "provider": "FDA",
    });
    let null = json!({
        "id": "live-test-clinical-report-null", "source": "null source", "clinicalStage": "UNKNOWN",
        "phaseFromSource": null, "evidenceType": "INDICATION", "title": "null report title",
        "trialStudyType": null, "trialDescription": null, "trialNumberOfArms": null,
        "trialStartDate": null, "trialLiterature": [], "trialOverallStatus": null,
        "trialWhyStopped": null, "trialPrimaryPurpose": null, "trialPhase": null,
        "trialStopReasonCategories": [], "trialSponsor": {"agencyClass": null, "name": null},
        "qualityControls": [],
        "diseases": [{"diseaseFromSource": "only disease from source"}],
        "drugs": [{"drugFromSource": "only drug from source", "drug": null}],
        "countries": [], "year": null, "sideEffects": [], "trialOfficialTitle": null,
        "url": "https://example.org/null-report", "origin": "CLINICAL_TRIAL", "provider": "ChEMBL",
    });
    (full, null)
}

/// `tests/live/fixtures/drug.jsonl`, `mechanism_of_action.jsonl`, `drug_warnings.jsonl` and
/// `clinical_indication_disease.jsonl` as the API serves them.
fn expected_drug() -> serde_json::Value {
    let parent = json!({
        "id": "CHEMBL9999998", "name": "LIVE TEST PARENT", "synonyms": [], "tradeNames": [],
        "drugType": "Small_molecule", "crossReferences": [], "maximumClinicalStage": "WITHDRAWAL",
        "description": "parent description", "molblock": null,
        "childMolecules": [{"id": "CHEMBL9999999"}],
    });
    let mechanisms = json!({
        "rows": [
            {
                "mechanismOfAction": "live test inhibitor", "actionType": "INHIBITOR",
                "targetName": "LTGENE1", "targets": [{"id": FIXTURE_TARGET}],
                "curation": "manual",
                "references": [{"ids": ["PMID-1"], "source": "PubMed",
                                "urls": ["https://example.org/moa"]}],
            },
            {
                "mechanismOfAction": "live test mechanism", "actionType": null,
                "targetName": null, "targets": [], "curation": "electronic", "references": [],
            },
        ],
        "uniqueActionTypes": ["CROSS_LINKING_AGENT", "INHIBITOR"],
        "uniqueTargetTypes": ["_3d_cell_culture", "single_protein"],
    });
    let warnings = json!({"count": 2, "rows": [
        {
            "toxicityClass": "hepatotoxicity", "chemblIds": ["CHEMBL9999999"], "country": "US",
            "description": "warning description", "id": 42,
            "references": [{"id": "warning-ref", "source": "FDA",
                            "url": "https://example.org/warning"}],
            "warningType": "Black Box Warning", "year": 2001, "efoTerm": "liver injury",
            "efoId": "EFO_0000002", "efoIdForWarningClass": "EFO_0000003",
        },
        {
            "toxicityClass": null, "chemblIds": [], "country": null, "description": null,
            "id": null, "references": [], "warningType": "Withdrawn", "year": null,
            "efoTerm": null, "efoId": null, "efoIdForWarningClass": null,
        },
    ]});
    json!({
        "id": "CHEMBL9999999", "name": "LIVE TEST DRUG",
        "synonyms": [{"label": "synonym label", "source": "synonym source"}],
        "tradeNames": [{"label": "trade name label", "source": "trade name source"}],
        "drugType": "Antibody_drug_conjugate",
        "crossReferences": [{"source": "DrugBank", "ids": ["DB00001", "DB00002"]}],
        "maximumClinicalStage": "APPROVAL", "description": "drug description",
        "molblock": "molblock text", "parentMolecule": parent, "childMolecules": [],
        "mechanismsOfAction": mechanisms,
        "drugWarnings": warnings,
        "indications": {"count": 1, "rows": [{
            "id": "live-test-ci", "maxClinicalStage": "PHASE_1_2", "drugName": "LIVE TEST DRUG",
            "diseaseName": null, "clinicalReportCount": 2, "drug": {"id": "CHEMBL9999999"},
            "clinicalReports": [
                {"id": "live-test-clinical-report"}, {"id": "live-test-clinical-report-null"},
            ],
        }]},
    })
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL and LIVE_FIXTURES=1"]
async fn clinical_fixtures_decode_to_inserted_values() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    assert!(with_fixtures(), "set LIVE_FIXTURES=1");
    let config = config(&url);
    load_fixtures(&ClickHouse::new(&config)).await;

    let state = AppState::new(config);
    let query = format!(
        "{{ full: clinicalReport(clinicalReportId: \"live-test-clinical-report\") {{ ...R }}
            null: clinicalReport(clinicalReportId: \"live-test-clinical-report-null\") {{ ...R }}
            clinicalReports(clinicalReportsIds: [\"live-test-clinical-report-null\",
                                                 \"live-test-clinical-report\"]) {{ id }}
            drug(chemblId: \"CHEMBL9999999\") {{
                ...D
                parentMolecule {{ ...D childMolecules {{ id }} }}
                childMolecules {{ id }}
                mechanismsOfAction {{
                    rows {{ mechanismOfAction actionType targetName targets {{ id }} curation
                            references {{ ids source urls }} }}
                    uniqueActionTypes uniqueTargetTypes
                }}
                drugWarnings {{ count rows {{ toxicityClass chemblIds country description id
                    references {{ id source url }} warningType year efoTerm efoId
                    efoIdForWarningClass }} }}
                indications {{ count rows {{ id maxClinicalStage drugName diseaseName
                    clinicalReportCount drug {{ id }} clinicalReports {{ id }} }} }}
            }}
            target(ensemblId: \"{FIXTURE_TARGET}\") {{
                drugAndClinicalCandidates {{ count rows {{ id maxClinicalStage drugName
                    clinicalReportCount diseases {{ diseaseFromSource }} drug {{ id }}
                    clinicalReports {{ id }} }} }}
            }}
        }}
        {CLINICAL_FIELDS}"
    );
    let response = state.api.execute(Request::new(query)).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json().expect("json");

    let (full, null) = expected_clinical_reports();
    assert_fields("full", &data["full"], &full);
    assert_eq!(data["full"], full);
    assert_fields("null", &data["null"], &null);
    assert_eq!(data["null"], null);
    assert_eq!(
        data["clinicalReports"],
        json!([{"id": "live-test-clinical-report-null"}, {"id": "live-test-clinical-report"}])
    );

    let expected = expected_drug();
    assert_fields("drug", &data["drug"], &expected);
    assert_eq!(data["drug"], expected);

    assert_eq!(
        data["target"]["drugAndClinicalCandidates"],
        json!({"count": 1, "rows": [{
            "id": "live-test-ct", "maxClinicalStage": "PHASE_4", "drugName": "LIVE TEST DRUG",
            "clinicalReportCount": 2,
            "diseases": [
                {"diseaseFromSource": "target disease from source"},
                {"diseaseFromSource": null},
            ],
            "drug": {"id": "CHEMBL9999999"},
            "clinicalReports": [
                {"id": "live-test-clinical-report"}, {"id": "live-test-clinical-report-null"},
            ],
        }]})
    );
}

// ---- disease, study, variant and other fixtures decode to the inserted values ----

#[cfg(feature = "product-ppp")]
const PPP_DISEASE_FIELDS: &str =
    "otarProjects { count rows { otarCode status projectName integratesInPPP reference } }";
#[cfg(not(feature = "product-ppp"))]
const PPP_DISEASE_FIELDS: &str = "";

const STUDY_FIELDS: &str = "fragment S on Study {
    id projectId studyType traitFromSource traitFromSourceMappedIds condition initialSampleSize
    nSamples nCases nControls cohorts discoverySamples { sampleSize ancestry }
    replicationSamples { sampleSize ancestry }
    ldPopulationStructure { ldPopulation relativeSampleSize }
    hasSumstats summarystatsLocation analysisFlags qualityControls
    SumstatQCValues { QCCheckName QCCheckValue }
    pubmedId publicationFirstAuthor publicationTitle publicationJournal publicationDate
}";

const VARIANT_FIELDS: &str = "variantId chromosome position referenceAllele alternateAllele
    variantEffect { method assessment score assessmentFlag normalisedScore target { id } }
    rsIds dbXrefs { id source } alleleFrequencies { populationName alleleFrequency }
    hgvsId variantDescription
    transcriptConsequences { aminoAcidChange uniprotAccessions isEnsemblCanonical codons
        distanceFromFootprint distanceFromTss impact lofteePrediction consequenceScore
        transcriptIndex transcriptId target { id } }
    transcriptConsequencesView { count rows { transcriptId target { id } impact
        lofteePrediction transcriptIndex consequenceScore codons aminoAcidChange } }
    enhancerToGene { count rows { chromosome start end biosampleFromSourceId intervalType
        distanceToTss score resourceScore { name value } datasourceId pmid studyId
        qualityControls target { id } } }
    byIntervalType: enhancerToGene(sort: [{key: INTERVAL_TYPE, direction: ASCENDING}]) {
        rows { intervalType } }
    enhancerSearch: enhancerToGene(search: {text: \"enhancer\"}) { count }
    pharmacogenomics { count rows { datasourceId datatypeId evidenceLevel genotype
        genotypeAnnotationText genotypeId haplotypeFromSourceId haplotypeId literature
        pgxCategory phenotypeFromSourceId phenotypeText studyId targetFromSourceId
        variantFunctionalConsequenceId variantRsId variantId isDirectTarget
        drugs { drugFromSource drugId drug { id } }
        variantAnnotation { baseAlleleOrGenotype comparisonAlleleOrGenotype directionality
            effect effectDescription effectType entity literature } } }
    pgxSearch: pharmacogenomics(search: {text: \"metabolism/pk\"}) { count }
    pgxDrugSearch: pharmacogenomics(search: {text: \"chembl9999999\"}) { count }";

fn expected_variant() -> serde_json::Value {
    let consequence = json!({
        "aminoAcidChange": "K12N", "uniprotAccessions": ["P99999"], "isEnsemblCanonical": true,
        "codons": "aaG/aaT", "distanceFromFootprint": 0, "distanceFromTss": 1234,
        "impact": "MODERATE", "lofteePrediction": "LC", "consequenceScore": 0.66,
        "transcriptIndex": 2, "transcriptId": "ENST99999999999",
        "target": {"id": FIXTURE_TARGET},
    });
    let null_consequence = json!({
        "aminoAcidChange": null, "uniprotAccessions": [], "isEnsemblCanonical": false,
        "codons": null, "distanceFromFootprint": -7, "distanceFromTss": -8,
        "impact": "MODIFIER", "lofteePrediction": null, "consequenceScore": 0.1,
        "transcriptIndex": 1, "transcriptId": "ENST99999999998", "target": null,
    });
    let pgx_annotated = json!({
        "datasourceId": "clinpgx", "datatypeId": "clinical_annotation", "evidenceLevel": "_2A",
        "genotype": "AT", "genotypeAnnotationText": "genotype annotation text",
        "genotypeId": "7_140000050_A_A,T", "haplotypeFromSourceId": "PA999",
        "haplotypeId": "LTGENE1*3", "literature": ["11111111"], "pgxCategory": "metabolism_pk",
        "phenotypeFromSourceId": "EFO_9999999", "phenotypeText": "phenotype text",
        "studyId": "PGX-STUDY-1", "targetFromSourceId": FIXTURE_TARGET,
        "variantFunctionalConsequenceId": "SO_0001583", "variantRsId": "rs999999",
        "variantId": "7_140000050_A_T", "isDirectTarget": true,
        "drugs": [
            {"drugFromSource": "live test drug from source", "drugId": "CHEMBL9999999",
             "drug": {"id": "CHEMBL9999999"}},
            {"drugFromSource": "unmapped drug", "drugId": null, "drug": null},
        ],
        "variantAnnotation": [
            {"baseAlleleOrGenotype": "AA", "comparisonAlleleOrGenotype": "AT",
             "directionality": "decreased", "effect": "effect",
             "effectDescription": "effect description", "effectType": "functional",
             "entity": "entity", "literature": "22222222"},
            {"baseAlleleOrGenotype": "TT", "comparisonAlleleOrGenotype": null,
             "directionality": null, "effect": null, "effectDescription": "other description",
             "effectType": "phenotype", "entity": null, "literature": "33333333"},
        ],
    });
    let pgx_bare = json!({
        "datasourceId": "clinpgx", "datatypeId": "clinical_annotation", "evidenceLevel": "_1B",
        "genotype": "TT", "genotypeAnnotationText": "null row text", "genotypeId": null,
        "haplotypeFromSourceId": null, "haplotypeId": null, "literature": [],
        "pgxCategory": "toxicity", "phenotypeFromSourceId": null, "phenotypeText": null,
        "studyId": "PGX-STUDY-2", "targetFromSourceId": null,
        "variantFunctionalConsequenceId": null, "variantRsId": null, "variantId": null,
        "isDirectTarget": false, "drugs": [], "variantAnnotation": [],
    });
    json!({
        "variantId": "7_140000050_A_T", "chromosome": "chr7", "position": 140_000_050,
        "referenceAllele": "A", "alternateAllele": "T",
        "variantEffect": [
            {"method": "GPN_Star", "assessment": "conserved", "score": 2.5,
             "assessmentFlag": "flag", "normalisedScore": 0.25,
             "target": {"id": FIXTURE_TARGET}},
            {"method": "VEP", "assessment": null, "score": null, "assessmentFlag": null,
             "normalisedScore": -0.5, "target": null},
        ],
        "rsIds": ["rs999999"],
        "dbXrefs": [
            {"id": "rs999999", "source": "ensembl_variation"},
            {"id": "VCV999", "source": "clinvar"},
        ],
        "alleleFrequencies": [
            {"populationName": "nfe_adj", "alleleFrequency": 0.125},
            {"populationName": "remaining_adj", "alleleFrequency": null},
        ],
        "hgvsId": "NC_000007.14:g.140000050A>T", "variantDescription": "variant description",
        // Sorted by `distanceFromFootprint` by default.
        "transcriptConsequences": [null_consequence, consequence],
        "transcriptConsequencesView": {"count": 2, "rows": [
            {"transcriptId": "ENST99999999998", "target": null, "impact": "MODIFIER",
             "lofteePrediction": null, "transcriptIndex": 1, "consequenceScore": 0.1,
             "codons": null, "aminoAcidChange": null},
            {"transcriptId": "ENST99999999999", "target": {"id": FIXTURE_TARGET},
             "impact": "MODERATE", "lofteePrediction": "LC", "transcriptIndex": 2,
             "consequenceScore": 0.66, "codons": "aaG/aaT", "aminoAcidChange": "K12N"},
        ]},
        "enhancerToGene": {"count": 2, "rows": [
            {"chromosome": "chr7", "start": 140_000_001, "end": 140_000_100,
             "biosampleFromSourceId": "EFO_SOURCE_1", "intervalType": "enhancer",
             "distanceToTss": -250, "score": 0.875,
             "resourceScore": [{"name": "ABC", "value": 0.5}], "datasourceId": "E2G",
             "pmid": "44444444", "studyId": "E2G-STUDY-1", "qualityControls": ["qc"],
             "target": {"id": FIXTURE_TARGET}},
            {"chromosome": "chr7", "start": 140_000_040, "end": 140_000_060,
             "biosampleFromSourceId": null, "intervalType": "promoter", "distanceToTss": 15,
             "score": 0.125, "resourceScore": [], "datasourceId": "E2G", "pmid": "55555555",
             "studyId": "E2G-STUDY-2", "qualityControls": [], "target": null},
        ]},
        "byIntervalType": {"rows": [{"intervalType": "enhancer"}, {"intervalType": "promoter"}]},
        "enhancerSearch": {"count": 1},
        "pharmacogenomics": {"count": 2, "rows": [pgx_bare, pgx_annotated]},
        "pgxSearch": {"count": 1},
        "pgxDrugSearch": {"count": 1},
    })
}

fn expected_studies() -> (serde_json::Value, serde_json::Value) {
    let full = json!({
        "id": "GCST99999999", "projectId": "GCST", "studyType": "gwas",
        "traitFromSource": "live test trait", "traitFromSourceMappedIds": ["EFO_9999999"],
        "condition": "stimulated", "initialSampleSize": "1,000 cases", "nSamples": 1000,
        "nCases": 100, "nControls": 900, "cohorts": ["UKB"],
        "discoverySamples": [
            {"sampleSize": 1000, "ancestry": "European"},
            {"sampleSize": 20, "ancestry": "South Asian"},
        ],
        "replicationSamples": [
            {"sampleSize": 300, "ancestry": "East Asian"},
            {"sampleSize": null, "ancestry": "African"},
        ],
        "ldPopulationStructure": [
            {"ldPopulation": "nfe", "relativeSampleSize": 0.75},
            {"ldPopulation": "csa", "relativeSampleSize": null},
        ],
        "hasSumstats": true, "summarystatsLocation": "gs://bucket/sumstats",
        "analysisFlags": ["analysis flag"], "qualityControls": ["qc flag"],
        "SumstatQCValues": [{"QCCheckName": "mean_beta", "QCCheckValue": 0.5}],
        "pubmedId": "99999999", "publicationFirstAuthor": "Author A",
        "publicationTitle": "publication title", "publicationJournal": "Journal J",
        "publicationDate": "2001-02-03",
    });
    let null = json!({
        "id": "GCST99999998", "projectId": "FINNGEN", "studyType": "eqtl",
        "traitFromSource": "null trait", "traitFromSourceMappedIds": [], "condition": null,
        "initialSampleSize": null, "nSamples": null, "nCases": null, "nControls": null,
        "cohorts": [], "discoverySamples": [], "replicationSamples": [],
        "ldPopulationStructure": [], "hasSumstats": null, "summarystatsLocation": null,
        "analysisFlags": [], "qualityControls": [], "SumstatQCValues": [], "pubmedId": null,
        "publicationFirstAuthor": null, "publicationTitle": null, "publicationJournal": null,
        "publicationDate": null,
    });
    (full, null)
}

fn expected_disease() -> serde_json::Value {
    let evidence = json!([
        {"aspect": "C", "bioCuration": "HPO:curator[2020-01-01]",
         "diseaseFromSourceId": "OMIM:999999", "diseaseFromSource": "disease from source",
         "diseaseName": "live test disease", "evidenceType": "PCS", "frequency": "HP_9999998",
         "qualifierNot": true, "references": ["PMID:1"], "sex": "FEMALE", "resource": "HPO",
         "frequencyHPO": {"id": "HP_9999998", "description": null}},
        {"aspect": null, "bioCuration": null, "diseaseFromSourceId": "ORPHA:1",
         "diseaseFromSource": "other source", "diseaseName": "other name",
         "evidenceType": null, "frequency": null, "qualifierNot": false, "references": [],
         "sex": null, "resource": "HPO", "frequencyHPO": null},
    ]);
    #[allow(unused_mut, reason = "extended on PPP only")]
    let mut disease = json!({
        "id": FIXTURE_DISEASE, "name": "live test disease", "description": "disease description",
        "isTherapeuticArea": false, "ancestors": ["EFO_9999998"], "descendants": [],
        "synonyms": [
            {"relation": "hasExactSynonym", "terms": ["exact synonym", null]},
            {"relation": "hasRelatedSynonym", "terms": ["related synonym"]},
        ],
        "obsoleteTerms": ["EFO_0000000"], "dbXRefs": ["MONDO:9999999", "OMIM:999999"],
        "studyIds": ["GCST99999999"], "indirectStudyIds": ["GCST99999998"],
        "parents": [{"id": "EFO_9999998", "description": null, "isTherapeuticArea": true}],
        "therapeuticAreas": [{"id": "EFO_9999998"}],
        "phenotypes": {"count": 1, "rows": [{
            "phenotypeHPO": {"id": "HP_9999999", "name": "live test phenotype",
                             "description": "phenotype description"},
            "evidence": evidence,
        }]},
        "literatureOcurrences": {"count": 1, "rows": [{"pmid": "77777777", "pmcid": "PMC777",
            "publicationDate": "2003-04-01"}], "stats": {"earliestPubDate": 2003}},
    });
    #[cfg(feature = "product-ppp")]
    {
        disease["otarProjects"] = json!({"count": 2, "rows": [
            {"otarCode": "OTAR999", "status": "Active", "projectName": "live test project",
             "integratesInPPP": true, "reference": "reference text"},
            {"otarCode": "OTAR998", "status": "Closed", "projectName": "other project",
             "integratesInPPP": false, "reference": ""},
        ]});
    }
    disease
}

/// Sorts `rows` by the field `key`, for tables read without an `ORDER BY`.
fn sorted_by(rows: &serde_json::Value, key: &str) -> serde_json::Value {
    let mut rows = rows.as_array().expect("rows").clone();
    rows.sort_by_key(|r| r[key].to_string());
    serde_json::Value::Array(rows)
}

fn expected_target_extras() -> serde_json::Value {
    json!({
        "baselineExpression": {"count": 2, "rows": [
            {"targetId": FIXTURE_TARGET, "targetFromSourceId": null,
             "tissueBiosampleFromSource": null, "celltypeBiosampleFromSource": null, "min": null,
             "q1": null, "median": null, "q3": null, "max": null, "distributionScore": 0.125,
             "specificityScore": null, "datasourceId": "PRIDE",
             "datatypeId": "mass_spectrometry_proteomics", "unit": "ppm", "qualityControls": []},
            {"targetId": FIXTURE_TARGET, "targetFromSourceId": "LTGENE1",
             "tissueBiosampleFromSource": "tissue from source",
             "celltypeBiosampleFromSource": "celltype from source", "min": 0.5, "q1": 1.5,
             "median": 2.5, "q3": 3.5, "max": 4.5, "distributionScore": 0.75,
             "specificityScore": 0.25, "datasourceId": "tabula_sapiens",
             "datatypeId": "scrna_seq", "unit": "TPM", "qualityControls": ["low counts"]},
        ]},
        "interactions": {"count": 2, "rows": [
            {"intA": "P99999", "intB": "Q99999", "intABiologicalRole": "bait",
             "intBBiologicalRole": "prey", "score": 0.75, "count": 3, "sourceDatabase": "signor",
             "speciesA": {"mnemonic": "human", "scientificName": "Homo sapiens", "taxonId": 9606},
             "speciesB": {"mnemonic": "mouse", "scientificName": "Mus musculus",
                          "taxonId": 10090},
             "evidences": [{"evidenceScore": 0.5, "hostOrganismTaxId": 9606,
                            "interactionIdentifier": "EBI-1", "pubmedId": "66666666",
                            "participantDetectionMethodB": [
                                {"miIdentifier": null, "shortName": null}]}]},
            {"intA": "P99999", "intB": "CHEBI:1", "intABiologicalRole": "unspecified role",
             "intBBiologicalRole": "unspecified role", "score": null, "count": 1,
             "sourceDatabase": "intact",
             "speciesA": {"mnemonic": "human", "scientificName": "Homo sapiens", "taxonId": 9606},
             "speciesB": {"mnemonic": "unknown", "scientificName": null, "taxonId": null},
             "evidences": []},
        ]},
        "literatureOcurrences": {"count": 2, "rows": [
            {"pmid": "77777777", "pmcid": "PMC777", "publicationDate": "2003-04-01"},
            {"pmid": "88888888", "pmcid": null, "publicationDate": null},
        ], "stats": {"earliestPubDate": 2003}},
        "datedLiterature": {"count": 1, "rows": [{"pmid": "77777777"}]},
        "drugLiterature": {"count": 1, "rows": [{"pmid": "77777777"}]},
    })
}

#[tokio::test]
#[ignore = "needs a ClickHouse database: set LIVE_CLICKHOUSE_URL and LIVE_FIXTURES=1"]
async fn other_fixtures_decode_to_inserted_values() {
    let url = std::env::var("LIVE_CLICKHOUSE_URL").expect("set LIVE_CLICKHOUSE_URL");
    assert!(with_fixtures(), "set LIVE_FIXTURES=1");
    let config = config(&url);
    load_fixtures(&ClickHouse::new(&config)).await;

    let state = AppState::new(config);
    let query = format!(
        "{{ disease(efoId: \"{FIXTURE_DISEASE}\") {{
                id name description isTherapeuticArea ancestors descendants
                synonyms {{ relation terms }} obsoleteTerms dbXRefs studyIds indirectStudyIds
                parents {{ id description isTherapeuticArea }} therapeuticAreas {{ id }}
                phenotypes {{ count rows {{
                    phenotypeHPO {{ id name description }}
                    evidence {{ aspect bioCuration diseaseFromSourceId diseaseFromSource
                        diseaseName evidenceType frequency qualifierNot references sex resource
                        frequencyHPO {{ id description }} }}
                }} }}
                literatureOcurrences {{ count rows {{ pmid pmcid publicationDate }}
                    stats {{ earliestPubDate }} }}
                {PPP_DISEASE_FIELDS}
            }}
            diseases(efoIds: [\"{FIXTURE_DISEASE}\"]) {{
                studiesView(enableIndirect: true) {{ count rows {{ id projectId traitFromSource
                    publicationFirstAuthor publicationDate publicationJournal nSamples cohorts
                    pubmedId ldPopulationStructure {{ ldPopulation relativeSampleSize }}
                    credibleSetsCount diseaseIds }} }}
            }}
            full: study(studyId: \"GCST99999999\") {{ ...S }}
            null: study(studyId: \"GCST99999998\") {{ ...S }}
            byDisease: studies(diseaseIds: [\"{FIXTURE_DISEASE}\"], enableIndirect: true) {{
                count rows {{ id }} }}
            variant(variantId: \"7_140000050_A_T\") {{ {VARIANT_FIELDS} }}
            interactionResources {{ databaseVersion sourceDatabase }}
            target(ensemblId: \"{FIXTURE_TARGET}\") {{
                baselineExpression {{ count rows {{ targetId targetFromSourceId
                    tissueBiosampleFromSource celltypeBiosampleFromSource min q1 median q3 max
                    distributionScore specificityScore datasourceId datatypeId unit
                    qualityControls }} }}
                interactions {{ count rows {{ intA intB intABiologicalRole intBBiologicalRole
                    score count sourceDatabase
                    speciesA {{ mnemonic scientificName taxonId }}
                    speciesB {{ mnemonic scientificName taxonId }}
                    evidences {{ evidenceScore hostOrganismTaxId interactionIdentifier pubmedId
                        participantDetectionMethodB {{ miIdentifier shortName }} }} }} }}
                literatureOcurrences {{ count rows {{ pmid pmcid publicationDate }}
                    stats {{ earliestPubDate }} }}
                datedLiterature: literatureOcurrences(startYear: 2000) {{ count rows {{ pmid }} }}
                drugLiterature: literatureOcurrences(additionalIds: [\"CHEMBL9999999\"]) {{
                    count rows {{ pmid }} }}
            }}
        }}
        {STUDY_FIELDS}"
    );
    let response = state.api.execute(Request::new(query)).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let mut data = response.data.into_json().expect("json");

    let disease = expected_disease();
    assert_fields("disease", &data["disease"], &disease);
    assert_eq!(data["disease"], disease);

    assert_eq!(
        data["diseases"]["studiesView"],
        json!({"count": 2, "rows": [
            {"id": "GCST99999999", "projectId": "GCST", "traitFromSource": "live test trait",
             "publicationFirstAuthor": "Author A", "publicationDate": "2001-02-03",
             "publicationJournal": "Journal J", "nSamples": 1000, "cohorts": ["UKB"],
             "pubmedId": "99999999",
             "ldPopulationStructure": [
                 {"ldPopulation": "nfe", "relativeSampleSize": 0.75},
                 {"ldPopulation": "csa", "relativeSampleSize": null},
             ],
             "credibleSetsCount": 1, "diseaseIds": [FIXTURE_DISEASE]},
            {"id": "GCST99999998", "projectId": "FINNGEN", "traitFromSource": "null trait",
             "publicationFirstAuthor": null, "publicationDate": null,
             "publicationJournal": null, "nSamples": null, "cohorts": [], "pubmedId": null,
             "ldPopulationStructure": [], "credibleSetsCount": 0,
             "diseaseIds": ["EFO_9999998"]},
        ]})
    );

    let (full, null) = expected_studies();
    assert_fields("full", &data["full"], &full);
    assert_eq!(data["full"], full);
    assert_fields("null", &data["null"], &null);
    assert_eq!(data["null"], null);
    assert_eq!(
        data["byDisease"],
        json!({"count": 2, "rows": [{"id": "GCST99999999"}, {"id": "GCST99999998"}]})
    );

    let variant = expected_variant();
    assert_fields("variant", &data["variant"], &variant);
    assert_eq!(data["variant"], variant);

    assert_eq!(
        data["interactionResources"],
        json!([{"databaseVersion": "2026-11", "sourceDatabase": "signor"}])
    );

    let rows = &data["target"]["baselineExpression"]["rows"];
    data["target"]["baselineExpression"]["rows"] = sorted_by(rows, "datasourceId");
    let expected = expected_target_extras();
    assert_fields("target", &data["target"], &expected);
    assert_eq!(data["target"], expected);
}
