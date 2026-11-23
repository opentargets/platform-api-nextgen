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
    Context, EmptyMutation, EmptySubscription, Object, PathSegment, Request, Schema, Variables,
};
use axum::body::Bytes;
use figment::{
    Figment,
    providers::{Format, Toml},
};
use platform_api::{
    AppState,
    config::Config,
    datasource::clickhouse::ClickHouse,
    entity::evidence::{Evidence, EvidenceKey, load_evidences},
    product::{CurrentProduct, Product},
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
    let mut insert = ch
        .client()
        .insert_formatted_with(format!("INSERT INTO `{table}` FORMAT JSONEachRow"));
    insert
        .send(Bytes::from(rows))
        .await
        .unwrap_or_else(|e| panic!("insert {}: {e}", jsonl_path.display()));
    insert
        .end()
        .await
        .unwrap_or_else(|e| panic!("insert {}: {e}", jsonl_path.display()));
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
