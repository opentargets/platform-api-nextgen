//! Loads the sample platform data used by the integration tests.
//!
//! The fixtures under `tests/fixtures/data` are a 10-row subset of the
//! Open Targets Platform release `platform-2612-3` output. They are used to
//! seed the ClickHouse and OpenSearch containers that back the API under test,
//! so the tests exercise the real query and data shapes rather than
//! hand-written fixtures.
//!
//! The directory layout mirrors the pipeline that produces the data: a dataset
//! lives under `view` or `output`, depending on which family it belongs to.
//!
//! # Why the mapping is not mechanical
//!
//! The Parquet view tables and the API's GraphQL types do not have the same
//! shape. The `clickhouse` crate matches tuple members *positionally*, so a
//! column that is added, dropped or reordered between the two produces
//! silently wrong data rather than an error. Every such difference is called
//! out below and must be kept in sync with `src/entity/target.rs` and
//! `tests/common/schema/targets.sql`.

use std::{fs::File, path::PathBuf};

use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Field, Row},
};
use serde_json::{Map, Value, json};

/// The directory holding the vendored sample data.
///
/// The fixtures are committed to this repository so the tests are hermetic and
/// do not depend on the surrounding workspace layout.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data")
}

/// Reads every row of a Parquet file as a [`Row`].
fn read_rows(relative_path: &str) -> Vec<Row> {
    let path = fixtures_dir().join(relative_path);
    let file = File::open(&path)
        .unwrap_or_else(|error| panic!("open sample data {}: {error}", path.display()));
    let reader = SerializedFileReader::new(file)
        .unwrap_or_else(|error| panic!("read sample data {}: {error}", path.display()));
    let mut rows = Vec::new();
    let iterator = reader
        .get_row_iter(None)
        .unwrap_or_else(|error| panic!("iterate sample data {}: {error}", path.display()));
    for row in iterator {
        rows.push(
            row.unwrap_or_else(|error| {
                panic!("read sample data row in {}: {error}", path.display())
            }),
        );
    }
    rows
}

/// Returns the value of a named column, if the row has it.
fn column<'a>(row: &'a Row, name: &str) -> Option<&'a Field> {
    row.get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value))
}

/// Returns the value of a named column, panicking when it is missing.
fn expect_column<'a>(row: &'a Row, name: &str) -> &'a Field {
    column(row, name).unwrap_or_else(|| panic!("sample data row is missing the '{name}' column"))
}

/// Reads a string column.
fn string(row: &Row, name: &str) -> String {
    match expect_column(row, name) {
        Field::Str(value) => value.clone(),
        other => panic!("column '{name}' is not a string: {other:?}"),
    }
}

/// Reads an optional string column, mapping an absent value to `None`.
fn optional_string(row: &Row, name: &str) -> Option<String> {
    match column(row, name) {
        Some(Field::Str(value)) => Some(value.clone()),
        Some(Field::Null) | None => None,
        Some(other) => panic!("column '{name}' is not a string: {other:?}"),
    }
}

/// Reads an optional boolean column, mapping an absent value to `None`.
fn optional_bool(row: &Row, name: &str) -> Option<bool> {
    match column(row, name) {
        Some(Field::Bool(value)) => Some(*value),
        Some(Field::Null) | None => None,
        Some(other) => panic!("column '{name}' is not a boolean: {other:?}"),
    }
}

/// Reads an optional double column, mapping an absent value to `None`.
fn optional_f64(row: &Row, name: &str) -> Option<f64> {
    match column(row, name) {
        Some(Field::Double(value)) => Some(*value),
        Some(Field::Float(value)) => Some(f64::from(*value)),
        Some(Field::Null) | None => None,
        Some(other) => panic!("column '{name}' is not a double: {other:?}"),
    }
}

/// Reads the elements of a list column, returning an empty list when absent.
fn list<'a>(row: &'a Row, name: &str) -> &'a [Field] {
    match column(row, name) {
        Some(Field::ListInternal(list)) => list.elements(),
        Some(Field::Null) | None => &[],
        Some(other) => panic!("column '{name}' is not a list: {other:?}"),
    }
}

/// Reads the elements of a list of structs column.
fn group_list<'a>(row: &'a Row, name: &str) -> Vec<&'a Row> {
    list(row, name)
        .iter()
        .map(|element| match element {
            Field::Group(group) => group,
            other => panic!("column '{name}' has a non-struct list element: {other:?}"),
        })
        .collect()
}

/// Reads a struct column, returning `None` when it is absent.
fn group<'a>(row: &'a Row, name: &str) -> Option<&'a Row> {
    match column(row, name) {
        Some(Field::Group(group)) => Some(group),
        Some(Field::Null) | None => None,
        Some(other) => panic!("column '{name}' is not a struct: {other:?}"),
    }
}

/// Builds a ClickHouse tuple from the named fields of a struct.
///
/// The members are ordered as given, because the `clickhouse` crate decodes
/// tuples positionally. A field that is absent becomes `null`.
fn tuple(row: &Row, members: &[&str]) -> Value {
    Value::Array(
        members
            .iter()
            .map(|member| {
                column(row, member).map_or(Value::Null, |value| match value {
                    Field::Str(text) => Value::String(text.clone()),
                    Field::Bool(flag) => Value::Bool(*flag),
                    Field::Byte(number) => json!(number),
                    Field::Short(number) => json!(number),
                    Field::Int(number) => json!(number),
                    Field::Long(number) => json!(number),
                    Field::UByte(number) => json!(number),
                    Field::UShort(number) => json!(number),
                    Field::UInt(number) => json!(number),
                    Field::ULong(number) => json!(number),
                    Field::Float(number) => json!(number),
                    Field::Double(number) => json!(number),
                    Field::Null => Value::Null,
                    other => panic!("unsupported value in tuple member '{member}': {other:?}"),
                })
            })
            .collect(),
    )
}

/// Builds a ClickHouse array of tuples from a list of structs.
fn tuple_list(rows: &[&Row], members: &[&str]) -> Value {
    Value::Array(rows.iter().map(|row| tuple(row, members)).collect())
}
/// Builds a ClickHouse array of strings from a list of scalars.
fn string_list(row: &Row, name: &str) -> Value {
    Value::Array(
        list(row, name)
            .iter()
            .map(|element| match element {
                Field::Str(text) => Value::String(text.clone()),
                other => panic!("column '{name}' has a non-string list element: {other:?}"),
            })
            .collect(),
    )
}

/// Converts a `target_view` Parquet row into the shape of the `targets`
/// ClickHouse table.
///
/// See the module documentation for the columns whose shape differs between the
/// Parquet view and the API.
fn target_row(row: &Row) -> Value {
    let mut object = Map::new();

    object.insert("id".to_string(), json!(string(row, "id")));
    object.insert(
        "alternativeGenes".to_string(),
        string_list(row, "alternativeGenes"),
    );
    object.insert(
        "approvedSymbol".to_string(),
        json!(string(row, "approvedSymbol")),
    );
    object.insert(
        "approvedName".to_string(),
        json!(string(row, "approvedName")),
    );
    object.insert("biotype".to_string(), json!(string(row, "biotype")));

    // The canonical transcript is a struct in the Parquet view and a tuple in
    // ClickHouse.
    object.insert(
        "canonicalTranscript".to_string(),
        group(row, "canonicalTranscript").map_or(Value::Null, |transcript| {
            tuple(transcript, &["id", "chromosome", "start", "end", "strand"])
        }),
    );

    // Chemical probes carry a nested list of URLs, so the tuple is assembled
    // member by member: `mechanismOfAction` and `origin` are lists of strings
    // and `urls` is a list of structs, all of which sit between the scalar
    // members.
    object.insert(
        "chemicalProbes".to_string(),
        Value::Array(
            group_list(row, "chemicalProbes")
                .iter()
                .map(|probe| {
                    Value::Array(vec![
                        json!(string(probe, "id")),
                        optional_string(probe, "control").into(),
                        optional_string(probe, "drugId").into(),
                        optional_string(probe, "drugFromSourceId").into(),
                        string_list(probe, "mechanismOfAction"),
                        json!(optional_bool(probe, "isHighQuality").unwrap_or_default()),
                        string_list(probe, "origin"),
                        optional_f64(probe, "probesDrugsScore").into(),
                        optional_f64(probe, "scoreInCells").into(),
                        optional_f64(probe, "scoreInOrganisms").into(),
                        json!(string(probe, "targetFromSourceId")),
                        tuple_list(&group_list(probe, "urls"), &["niceName", "url"]),
                    ])
                })
                .collect(),
        ),
    );

    object.insert(
        "dbXrefs".to_string(),
        tuple_list(&group_list(row, "dbXrefs"), &["id", "source"]),
    );
    object.insert(
        "functionDescriptions".to_string(),
        string_list(row, "functionDescriptions"),
    );
    object.insert(
        "constraint".to_string(),
        tuple_list(
            &group_list(row, "constraint"),
            &[
                "constraintType",
                "exp",
                "obs",
                "oe",
                "oeLower",
                "oeUpper",
                "score",
                "upperBin",
                "upperBin6",
                "upperRank",
            ],
        ),
    );
    object.insert(
        "genomicLocation".to_string(),
        group(row, "genomicLocation").map_or(Value::Null, |location| {
            tuple(location, &["chromosome", "start", "end", "strand"])
        }),
    );

    // The Parquet view has six GO fields (`id`, `source`, `evidence`, `aspect`,
    // `geneProduct`, `ecoId`) while the API has five, in a different order and
    // without `ecoId`.
    object.insert(
        "go".to_string(),
        Value::Array(
            group_list(row, "go")
                .iter()
                .map(|annotation| {
                    Value::Array(vec![
                        json!(string(annotation, "id")),
                        json!(string(annotation, "aspect")),
                        json!(string(annotation, "evidence")),
                        json!(string(annotation, "geneProduct")),
                        json!(string(annotation, "source")),
                    ])
                })
                .collect(),
        ),
    );

    // Hallmarks are a struct of two arrays of structs.
    object.insert(
        "hallmarks".to_string(),
        match group(row, "hallmarks") {
            Some(hallmarks) => Value::Array(vec![
                tuple_list(
                    &group_list(hallmarks, "cancerHallmarks"),
                    &["description", "impact", "label", "pmid"],
                ),
                tuple_list(
                    &group_list(hallmarks, "attributes"),
                    &["name", "description", "pmid"],
                ),
            ]),
            None => Value::Array(vec![Value::Array(Vec::new()), Value::Array(Vec::new())]),
        },
    );

    // The Parquet view has nine homologue fields (adding `priority`) in a
    // different order than the API's eight.
    object.insert(
        "homologues".to_string(),
        Value::Array(
            group_list(row, "homologues")
                .iter()
                .map(|homologue| {
                    Value::Array(vec![
                        json!(string(homologue, "homologyType")),
                        json!(optional_f64(homologue, "queryPercentageIdentity")),
                        json!(string(homologue, "speciesId")),
                        json!(string(homologue, "speciesName")),
                        json!(string(homologue, "targetGeneId")),
                        json!(string(homologue, "targetGeneSymbol")),
                        json!(optional_f64(homologue, "targetPercentageIdentity")),
                        optional_string(homologue, "isHighConfidence").into(),
                    ])
                })
                .collect(),
        ),
    );

    // The Parquet view orders the pathway fields as `pathwayId`, `pathway`,
    // `topLevelTerm`; the API expects `pathway` first.
    object.insert(
        "pathways".to_string(),
        Value::Array(
            group_list(row, "pathways")
                .iter()
                .map(|pathway| {
                    Value::Array(vec![
                        json!(string(pathway, "pathway")),
                        json!(string(pathway, "pathwayId")),
                        json!(string(pathway, "topLevelTerm")),
                    ])
                })
                .collect(),
        ),
    );

    object.insert(
        "proteinIds".to_string(),
        tuple_list(&group_list(row, "proteinIds"), &["id", "source"]),
    );

    // Safety liabilities are a tuple whose first and last members are lists.
    object.insert(
        "safetyLiabilities".to_string(),
        Value::Array(
            group_list(row, "safetyLiabilities")
                .iter()
                .map(|liability| {
                    Value::Array(vec![
                        tuple_list(
                            &group_list(liability, "biosamples"),
                            &[
                                "tissueLabel",
                                "tissueId",
                                "cellLabel",
                                "cellFormat",
                                "cellId",
                            ],
                        ),
                        json!(string(liability, "datasource")),
                        tuple_list(&group_list(liability, "effects"), &["direction", "dosing"]),
                        optional_string(liability, "event").into(),
                        optional_string(liability, "eventId").into(),
                        optional_string(liability, "literature").into(),
                        optional_string(liability, "url").into(),
                        tuple_list(
                            &group_list(liability, "studies"),
                            &["name", "description", "type"],
                        ),
                    ])
                })
                .collect(),
        ),
    );

    // These two columns are not part of the Parquet view: they are derived by
    // the output pipeline by joining the credible sets of each target.
    object.insert("studyLocusIds".to_string(), Value::Array(Vec::new()));
    object.insert("pharmacogenomicsIds".to_string(), Value::Array(Vec::new()));

    object.insert(
        "subcellularLocations".to_string(),
        tuple_list(
            &group_list(row, "subcellularLocations"),
            &["location", "source", "termSL", "labelSL", "targetModifier"],
        ),
    );
    object.insert(
        "synonyms".to_string(),
        tuple_list(&group_list(row, "synonyms"), &["label", "source"]),
    );
    object.insert(
        "symbolSynonyms".to_string(),
        tuple_list(&group_list(row, "symbolSynonyms"), &["label", "source"]),
    );
    object.insert(
        "nameSynonyms".to_string(),
        tuple_list(&group_list(row, "nameSynonyms"), &["label", "source"]),
    );
    object.insert(
        "obsoleteSymbols".to_string(),
        tuple_list(&group_list(row, "obsoleteSymbols"), &["label", "source"]),
    );
    object.insert(
        "obsoleteNames".to_string(),
        tuple_list(&group_list(row, "obsoleteNames"), &["label", "source"]),
    );
    object.insert(
        "targetClass".to_string(),
        tuple_list(&group_list(row, "targetClass"), &["id", "label", "level"]),
    );
    object.insert(
        "tractability".to_string(),
        tuple_list(
            &group_list(row, "tractability"),
            &["id", "modality", "value"],
        ),
    );
    object.insert(
        "transcriptIds".to_string(),
        string_list(row, "transcriptIds"),
    );

    // The boolean members of a transcript are optional in the API.
    object.insert(
        "transcripts".to_string(),
        Value::Array(
            group_list(row, "transcripts")
                .iter()
                .map(|transcript| {
                    Value::Array(vec![
                        json!(string(transcript, "transcriptId")),
                        json!(string(transcript, "biotype")),
                        optional_bool(transcript, "isEnsemblCanonical").into(),
                        optional_string(transcript, "uniprotId").into(),
                        optional_bool(transcript, "isUniprotReviewed").into(),
                        optional_string(transcript, "translationId").into(),
                        optional_string(transcript, "alphafoldId").into(),
                        optional_string(transcript, "uniprotIsoformId").into(),
                    ])
                })
                .collect(),
        ),
    );

    Value::Object(object)
}

/// Returns the `targets` rows of the sample data, ready to be inserted into
/// ClickHouse.
pub fn targets() -> Vec<Value> {
    read_rows("view/target_view/00000000.parquet")
        .iter()
        .map(target_row)
        .collect()
}

/// Returns the search documents of the sample data, ready to be indexed into
/// OpenSearch.
pub fn search_targets() -> Vec<Value> {
    read_rows("view/search_target/00000000.parquet")
        .iter()
        .map(|row| {
            let mut object = Map::new();
            object.insert("id".to_string(), json!(string(row, "id")));
            object.insert("name".to_string(), json!(string(row, "name")));
            object.insert(
                "description".to_string(),
                match column(row, "description") {
                    Some(Field::Str(text)) => json!(text),
                    _ => Value::Null,
                },
            );
            object.insert("entity".to_string(), json!(string(row, "entity")));
            object.insert("category".to_string(), string_list(row, "category"));
            object.insert("keywords".to_string(), string_list(row, "keywords"));
            object.insert("prefixes".to_string(), string_list(row, "prefixes"));
            object.insert("ngrams".to_string(), string_list(row, "ngrams"));
            object.insert("terms".to_string(), string_list(row, "terms"));
            object.insert("terms25".to_string(), string_list(row, "terms25"));
            object.insert("terms5".to_string(), string_list(row, "terms5"));
            object.insert(
                "multiplier".to_string(),
                json!(optional_f64(row, "multiplier").unwrap_or_default()),
            );
            Value::Object(object)
        })
        .collect()
}

/// Returns the identifier of the first target of the sample data.
pub fn first_target_id() -> String {
    let rows = read_rows("view/target_view/00000000.parquet");
    string(rows.first().expect("sample data has no targets"), "id")
}

/// Returns the identifier of the second target of the sample data.
pub fn second_target_id() -> String {
    let rows = read_rows("view/target_view/00000000.parquet");
    string(
        rows.get(1).expect("sample data has fewer than two targets"),
        "id",
    )
}

/// Returns the symbol of the first target of the sample data.
pub fn first_target_symbol() -> String {
    let rows = read_rows("view/target_view/00000000.parquet");
    string(
        rows.first().expect("sample data has no targets"),
        "approvedSymbol",
    )
}

/// Returns the name of the first target of the sample data.
pub fn first_target_name() -> String {
    let rows = read_rows("view/target_view/00000000.parquet");
    string(
        rows.first().expect("sample data has no targets"),
        "approvedName",
    )
}

/// Returns the symbol of the second target of the sample data.
pub fn second_target_symbol() -> String {
    let rows = read_rows("view/target_view/00000000.parquet");
    string(
        rows.get(1).expect("sample data has fewer than two targets"),
        "approvedSymbol",
    )
}

/// Returns the name of the second target of the sample data.
pub fn second_target_name() -> String {
    let rows = read_rows("view/target_view/00000000.parquet");
    string(
        rows.get(1).expect("sample data has fewer than two targets"),
        "approvedName",
    )
}

/// Returns the identifier of the first search document of the sample data.
pub fn first_search_target_id() -> String {
    let rows = read_rows("view/search_target/00000000.parquet");
    string(
        rows.first().expect("sample data has no search targets"),
        "id",
    )
}

/// Returns the name of the first search document of the sample data.
pub fn first_search_target_name() -> String {
    let rows = read_rows("view/search_target/00000000.parquet");
    string(
        rows.first().expect("sample data has no search targets"),
        "name",
    )
}
