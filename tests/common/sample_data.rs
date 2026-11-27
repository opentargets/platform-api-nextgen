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

/// Reads an optional 32-bit unsigned integer column, mapping an absent value to
/// `None`.
fn optional_u32(row: &Row, name: &str) -> Option<u32> {
    match column(row, name) {
        Some(Field::Int(value)) => Some(u32::try_from(*value).unwrap_or_default()),
        Some(Field::UInt(value)) => Some(*value),
        Some(Field::Long(value)) => Some(u32::try_from(*value).unwrap_or_default()),
        Some(Field::ULong(value)) => Some(u32::try_from(*value).unwrap_or_default()),
        Some(Field::Null) | None => None,
        Some(other) => panic!("column '{name}' is not an unsigned integer: {other:?}"),
    }
}

/// Reads an optional 32-bit signed integer column, mapping an absent value to
/// `None`.
fn optional_i32(row: &Row, name: &str) -> Option<i32> {
    match column(row, name) {
        Some(Field::Int(value)) => Some(*value),
        Some(Field::Short(value)) => Some(i32::from(*value)),
        Some(Field::Long(value)) => Some(i32::try_from(*value).unwrap_or_default()),
        Some(Field::Null) | None => None,
        Some(other) => panic!("column '{name}' is not an integer: {other:?}"),
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

/// Builds a ClickHouse tuple whose last member is a list of strings.
///
/// The cross references of a drug are a source and the identifiers the source
/// gives it, so the identifiers are a list inside the tuple.
fn tuple_with_string_list(row: &Row, members: &[&str]) -> Value {
    let list_member = members.last().expect("the tuple has a member");
    let values = members
        .iter()
        .map(|member| {
            if *member == *list_member {
                return match column(row, member) {
                    Some(Field::ListInternal(list)) => Value::Array(
                        list.elements()
                            .iter()
                            .map(|element| match element {
                                Field::Str(text) => Value::String(text.clone()),
                                other => {
                                    panic!("column '{member}' has a non-string element: {other:?}")
                                }
                            })
                            .collect(),
                    ),
                    Some(Field::Null) | None => Value::Array(Vec::new()),
                    Some(other) => panic!("column '{member}' is not a list: {other:?}"),
                };
            }
            column(row, member).map_or(Value::Null, |value| match value {
                Field::Str(text) => Value::String(text.clone()),
                Field::Null => Value::Null,
                other => panic!("unsupported value in tuple member '{member}': {other:?}"),
            })
        })
        .collect();
    Value::Array(values)
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

/// Converts a `drug_molecule` Parquet row into the shape of the `drug`
/// ClickHouse table.
///
/// The Parquet dataset and the API's `Drug` type have the same shape, so the
/// mapping is mechanical. The identifiers of the pharmacogenomics are not in
/// the dataset, so they are empty.
fn drug_row(row: &Row) -> Value {
    let mut object = Map::new();

    object.insert("id".to_string(), json!(string(row, "id")));
    object.insert("name".to_string(), json!(string(row, "name")));
    object.insert(
        "synonyms".to_string(),
        tuple_list(&group_list(row, "synonyms"), &["label", "source"]),
    );
    object.insert(
        "tradeNames".to_string(),
        tuple_list(&group_list(row, "tradeNames"), &["label", "source"]),
    );
    object.insert("drugType".to_string(), json!(string(row, "drugType")));
    object.insert(
        "crossReferences".to_string(),
        Value::Array(
            group_list(row, "crossReferences")
                .iter()
                .map(|reference| tuple_with_string_list(reference, &["source", "ids"]))
                .collect(),
        ),
    );
    object.insert(
        "parentId".to_string(),
        optional_string(row, "parentId").into(),
    );
    // The dataset leaves the list empty for the drugs without derivatives.
    object.insert(
        "childChemblIds".to_string(),
        Value::Array(
            list(row, "childChemblIds")
                .iter()
                .map(|element| match element {
                    Field::Str(text) => json!(text),
                    Field::Null => Value::Null,
                    other => panic!("column 'childChemblIds' has a non-string element: {other:?}"),
                })
                .collect(),
        ),
    );
    object.insert(
        "maximumClinicalStage".to_string(),
        json!(string(row, "maximumClinicalStage")),
    );
    object.insert(
        "description".to_string(),
        optional_string(row, "description").into(),
    );
    object.insert(
        "molblock".to_string(),
        optional_string(row, "molblock").into(),
    );
    object.insert("pharmacogenomicsIds".to_string(), Value::Array(Vec::new()));

    Value::Object(object)
}

/// Returns the `drug` rows of the sample data, ready to be inserted into
/// ClickHouse.
pub fn drugs() -> Vec<Value> {
    read_rows("output/drug_molecule/00000000.parquet")
        .iter()
        .map(drug_row)
        .collect()
}

/// Converts a `disease` Parquet row into the shape of the `disease` ClickHouse
/// table.
///
/// The Parquet dataset and the API's `Disease` type differ in two places:
///
/// - The synonyms are in four separate lists, one per relation, while the API expects a list of
///   relations, each with its terms. The production pipeline makes the same transformation in SQL,
///   with `tupleToNameValuePairs`.
/// - The flag telling whether the disease is a therapeutic area is inside the `ontology` struct,
///   while the API expects it on its own.
///
/// The identifiers of the studies and the locations of a disease come from
/// joins with the studies in production, so they are empty here.
fn disease_row(row: &Row) -> Value {
    let mut object = Map::new();

    object.insert("id".to_string(), json!(string(row, "id")));
    object.insert("name".to_string(), json!(string(row, "name")));
    object.insert(
        "description".to_string(),
        optional_string(row, "description").into(),
    );
    object.insert(
        "isTherapeuticArea".to_string(),
        json!(
            group(row, "ontology")
                .and_then(|ontology| optional_bool(ontology, "isTherapeuticArea"))
                .unwrap_or_default()
        ),
    );
    object.insert(
        "therapeuticAreas".to_string(),
        string_list(row, "therapeuticAreas"),
    );
    object.insert("parents".to_string(), string_list(row, "parents"));
    object.insert("children".to_string(), string_list(row, "children"));
    object.insert("ancestors".to_string(), string_list(row, "ancestors"));
    object.insert("descendants".to_string(), string_list(row, "descendants"));

    // The synonyms are in four lists, one per relation. The API expects a list
    // of relations, each with the terms that have it.
    let relations = [
        ("hasExactSynonym", "exactSynonyms"),
        ("hasRelatedSynonym", "relatedSynonyms"),
        ("hasNarrowSynonym", "narrowSynonyms"),
        ("hasBroadSynonym", "broadSynonyms"),
    ];
    object.insert(
        "synonyms".to_string(),
        Value::Array(
            relations
                .iter()
                .filter_map(|(relation, column_name)| {
                    let terms = list(row, column_name);
                    (!terms.is_empty()).then(|| {
                        json!([
                            relation,
                            Value::Array(
                                terms
                                    .iter()
                                    .map(|term| match term {
                                        Field::Str(text) => json!(text),
                                        Field::Null => Value::Null,
                                        other => panic!(
                                            "column '{column_name}' has a non-string element: {other:?}"
                                        ),
                                    })
                                    .collect(),
                            ),
                        ])
                    })
                })
                .collect(),
        ),
    );

    object.insert(
        "obsoleteTerms".to_string(),
        string_list(row, "obsoleteTerms"),
    );
    object.insert("dbXRefs".to_string(), string_list(row, "dbXRefs"));
    object.insert("directLocationIds".to_string(), Value::Array(Vec::new()));
    object.insert("indirectLocationIds".to_string(), Value::Array(Vec::new()));
    // The studies of a disease come from the studies table in production, by
    // joining the two on the diseases a study is about. The two subsets of the
    // sample data are disjoint, so the first disease is given the first study
    // of the sample data. This is what lets the tests check that the studies
    // of a disease are found.
    object.insert(
        "studyIds".to_string(),
        if string(row, "id") == first_disease_id() {
            Value::Array(vec![json!(first_study_id())])
        } else {
            Value::Array(Vec::new())
        },
    );
    object.insert("indirectStudyIds".to_string(), Value::Array(Vec::new()));

    Value::Object(object)
}

/// Returns the `disease` rows of the sample data, ready to be inserted into
/// ClickHouse.
pub fn diseases() -> Vec<Value> {
    read_rows("output/disease/00000000.parquet")
        .iter()
        .map(disease_row)
        .collect()
}

/// Returns the number of a type of study.
///
/// The API decodes the type of a study as a number, while the dataset holds the
/// name of it.
fn study_type_code(name: &str) -> i8 {
    match name {
        "tuqtl" => 1,
        "pqtl" => 2,
        "eqtl" => 3,
        "sqtl" => 4,
        "sctuqtl" => 5,
        "scpqtl" => 6,
        "sceqtl" => 7,
        "scsqtl" => 8,
        "gwas" => 9,
        other => panic!("unknown study type '{other}'"),
    }
}

/// Converts a `study` Parquet row into the shape of the `study` ClickHouse
/// table.
///
/// The members of the samples are in a different order than the API expects:
/// the dataset has the size of the sample first, the API has the ancestry.
fn study_row(row: &Row) -> Value {
    let mut object = Map::new();

    object.insert("studyId".to_string(), json!(string(row, "studyId")));
    object.insert("projectId".to_string(), json!(string(row, "projectId")));
    // The API decodes the type of a study as a number.
    object.insert(
        "studyType".to_string(),
        json!(study_type_code(&string(row, "studyType"))),
    );
    object.insert(
        "traitFromSource".to_string(),
        json!(string(row, "traitFromSource")),
    );
    object.insert(
        "traitFromSourceMappedIds".to_string(),
        string_list(row, "traitFromSourceMappedIds"),
    );
    object.insert(
        "condition".to_string(),
        optional_string(row, "condition").into(),
    );
    object.insert(
        "initialSampleSize".to_string(),
        optional_string(row, "initialSampleSize").into(),
    );
    object.insert("nSamples".to_string(), optional_u32(row, "nSamples").into());
    object.insert("nCases".to_string(), optional_u32(row, "nCases").into());
    object.insert(
        "nControls".to_string(),
        optional_u32(row, "nControls").into(),
    );
    object.insert("cohorts".to_string(), string_list(row, "cohorts"));

    // The samples hold the ancestry and the size of the sample, in that order.
    let samples = |name: &str| {
        Value::Array(
            group_list(row, name)
                .iter()
                .map(|sample| {
                    json!([
                        string(sample, "ancestry"),
                        optional_u32(sample, "sampleSize").unwrap_or_default(),
                    ])
                })
                .collect(),
        )
    };
    object.insert("discoverySamples".to_string(), samples("discoverySamples"));
    object.insert(
        "replicationSamples".to_string(),
        samples("replicationSamples"),
    );

    object.insert(
        "ldPopulationStructure".to_string(),
        Value::Array(
            group_list(row, "ldPopulationStructure")
                .iter()
                .map(|population| {
                    json!([
                        string(population, "ldPopulation"),
                        optional_f64(population, "relativeSampleSize")
                            .map_or(Value::Null, |value| json!(value)),
                    ])
                })
                .collect(),
        ),
    );
    object.insert(
        "hasSumstats".to_string(),
        optional_bool(row, "hasSumstats").into(),
    );
    object.insert(
        "summarystatsLocation".to_string(),
        optional_string(row, "summarystatsLocation").into(),
    );
    object.insert(
        "analysisFlags".to_string(),
        string_list(row, "analysisFlags"),
    );
    object.insert(
        "qualityControls".to_string(),
        string_list(row, "qualityControls"),
    );
    object.insert(
        "sumstatQCValues".to_string(),
        Value::Array(
            group_list(row, "sumstatQCValues")
                .iter()
                .map(|value| {
                    json!([
                        string(value, "qcCheckName"),
                        optional_f64(value, "qcCheckValue").unwrap_or_default(),
                    ])
                })
                .collect(),
        ),
    );
    object.insert(
        "pubmedId".to_string(),
        optional_string(row, "pubmedId").into(),
    );
    object.insert(
        "publicationFirstAuthor".to_string(),
        optional_string(row, "publicationFirstAuthor").into(),
    );
    object.insert(
        "publicationTitle".to_string(),
        optional_string(row, "publicationTitle").into(),
    );
    object.insert(
        "publicationJournal".to_string(),
        optional_string(row, "publicationJournal").into(),
    );
    object.insert(
        "publicationDate".to_string(),
        optional_string(row, "publicationDate").into(),
    );
    object.insert("diseaseIds".to_string(), string_list(row, "diseaseIds"));

    // The credible sets of a study come from the credible sets table in
    // production. The tests do not seed that table, so they are empty.
    object.insert("studyLocusIds".to_string(), Value::Array(Vec::new()));
    object.insert("geneId".to_string(), optional_string(row, "geneId").into());
    object.insert(
        "biosampleFromSourceId".to_string(),
        optional_string(row, "biosampleFromSourceId").into(),
    );
    object.insert(
        "backgroundTraitFromSourceMappedIds".to_string(),
        string_list(row, "backgroundTraitFromSourceMappedIds"),
    );

    Value::Object(object)
}

/// Returns the `study` rows of the sample data, ready to be inserted into
/// ClickHouse.
pub fn studies() -> Vec<Value> {
    read_rows("output/study/00000000.parquet")
        .iter()
        .map(study_row)
        .collect()
}

/// Converts a `clinical_report` Parquet row into the shape of the
/// `clinical_report` ClickHouse table.
///
/// The Parquet dataset and the API's `ClinicalReport` type have the same
/// shape, so the mapping is mechanical.
fn clinical_report_row(row: &Row) -> Value {
    let mut object = Map::new();

    object.insert("id".to_string(), json!(string(row, "id")));
    object.insert("source".to_string(), json!(string(row, "source")));
    object.insert(
        "clinicalStage".to_string(),
        json!(string(row, "clinicalStage")),
    );
    object.insert("origin".to_string(), json!(string(row, "origin")));
    object.insert(
        "phaseFromSource".to_string(),
        optional_string(row, "phaseFromSource").into(),
    );
    object.insert("provider".to_string(), json!(string(row, "provider")));
    object.insert("type".to_string(), optional_string(row, "type").into());
    object.insert("title".to_string(), optional_string(row, "title").into());
    object.insert(
        "trialStudyType".to_string(),
        optional_string(row, "trialStudyType").into(),
    );
    object.insert(
        "trialDescription".to_string(),
        optional_string(row, "trialDescription").into(),
    );
    object.insert(
        "trialNumberOfArms".to_string(),
        optional_i32(row, "trialNumberOfArms").into(),
    );
    object.insert(
        "trialStartDate".to_string(),
        optional_string(row, "trialStartDate").into(),
    );
    object.insert(
        "trialLiterature".to_string(),
        tuple_list(&group_list(row, "trialLiterature"), &["id", "type"]),
    );
    object.insert(
        "trialOverallStatus".to_string(),
        optional_string(row, "trialOverallStatus").into(),
    );
    object.insert(
        "trialWhyStopped".to_string(),
        optional_string(row, "trialWhyStopped").into(),
    );
    object.insert(
        "trialPrimaryPurpose".to_string(),
        optional_string(row, "trialPrimaryPurpose").into(),
    );
    object.insert(
        "trialPhase".to_string(),
        optional_string(row, "trialPhase").into(),
    );
    object.insert(
        "trialStopReasonCategories".to_string(),
        string_list(row, "trialStopReasonCategories"),
    );
    object.insert(
        "trialSponsor".to_string(),
        match group(row, "trialSponsor") {
            Some(sponsor) => json!([
                optional_string(sponsor, "agencyClass").map_or(Value::Null, |value| json!(value)),
                optional_string(sponsor, "name").map_or(Value::Null, |value| json!(value)),
            ]),
            None => Value::Null,
        },
    );
    object.insert(
        "qualityControls".to_string(),
        string_list(row, "qualityControls"),
    );
    object.insert(
        "diseases".to_string(),
        tuple_list(
            &group_list(row, "diseases"),
            &["diseaseFromSource", "diseaseId"],
        ),
    );
    object.insert(
        "drugs".to_string(),
        tuple_list(&group_list(row, "drugs"), &["drugFromSource", "drugId"]),
    );
    object.insert("countries".to_string(), string_list(row, "countries"));
    object.insert("year".to_string(), optional_i32(row, "year").into());
    object.insert(
        "sideEffects".to_string(),
        Value::Array(
            group_list(row, "sideEffects")
                .iter()
                .map(|effect| {
                    json!([
                        optional_string(effect, "diseaseId")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_string(effect, "diseaseFromSource")
                            .map_or(Value::Null, |value| json!(value)),
                    ])
                })
                .collect(),
        ),
    );
    object.insert(
        "trialOfficialTitle".to_string(),
        optional_string(row, "trialOfficialTitle").into(),
    );
    object.insert("url".to_string(), optional_string(row, "url").into());

    Value::Object(object)
}

/// Returns the `clinical_report` rows of the sample data, ready to be inserted
/// into ClickHouse.
pub fn clinical_reports() -> Vec<Value> {
    read_rows("output/clinical_report/00000000.parquet")
        .iter()
        .map(clinical_report_row)
        .collect()
}

/// Returns the number of a chromosome.
///
/// The API decodes the chromosome of a variant as a number, while the dataset
/// holds its name.
fn chromosome_code(name: &str) -> i8 {
    match name {
        "X" => 23,
        "Y" => 24,
        "MT" => 25,
        other => other
            .parse()
            .unwrap_or_else(|_| panic!("unknown chromosome {other}")),
    }
}

/// Converts a `variant` Parquet row into the shape of the `variants`
/// ClickHouse table.
///
/// The transcript consequences of the dataset hold more members than the API
/// expects, and in a different order, so they are picked by name. The two
/// predictions the dataset does not hold are left empty.
fn variant_row(row: &Row) -> Value {
    let mut object = Map::new();

    object.insert("variantId".to_string(), json!(string(row, "variantId")));
    object.insert(
        "chromosome".to_string(),
        json!(chromosome_code(&string(row, "chromosome"))),
    );
    object.insert(
        "position".to_string(),
        json!(u64::from(optional_u32(row, "position").unwrap_or_default(),)),
    );
    object.insert(
        "referenceAllele".to_string(),
        json!(string(row, "referenceAllele")),
    );
    object.insert(
        "alternateAllele".to_string(),
        json!(string(row, "alternateAllele")),
    );
    object.insert(
        "variantEffect".to_string(),
        tuple_list(
            &group_list(row, "variantEffect"),
            &[
                "method",
                "assessment",
                "score",
                "assessmentFlag",
                "targetId",
                "normalisedScore",
            ],
        ),
    );

    // The transcript consequences are picked by name: the dataset holds more
    // members than the API expects, in a different order.
    object.insert(
        "transcriptConsequences".to_string(),
        Value::Array(
            group_list(row, "transcriptConsequences")
                .iter()
                .map(|consequence| {
                    json!([
                        string_list(consequence, "variantFunctionalConsequenceIds"),
                        optional_string(consequence, "aminoAcidChange")
                            .map_or(Value::Null, |value| json!(value)),
                        string_list(consequence, "uniprotAccessions"),
                        optional_bool(consequence, "isEnsemblCanonical").unwrap_or_default(),
                        optional_string(consequence, "codons")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_i32(consequence, "distanceFromFootprint").unwrap_or_default(),
                        optional_i32(consequence, "distanceFromTss").unwrap_or_default(),
                        optional_string(consequence, "targetId")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_string(consequence, "impact")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_string(consequence, "transcriptId")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_string(consequence, "lofteePrediction")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_f64(consequence, "siftPrediction")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_f64(consequence, "polyphenPrediction")
                            .map_or(Value::Null, |value| json!(value)),
                        optional_u32(consequence, "transcriptIndex").unwrap_or_default(),
                        optional_f64(consequence, "consequenceScore").unwrap_or_default(),
                    ])
                })
                .collect(),
        ),
    );

    object.insert("rsIds".to_string(), string_list(row, "rsIds"));
    object.insert(
        "dbXrefs".to_string(),
        tuple_list(&group_list(row, "dbXrefs"), &["id", "source"]),
    );
    object.insert(
        "alleleFrequencies".to_string(),
        tuple_list(
            &group_list(row, "alleleFrequencies"),
            &["populationName", "alleleFrequency"],
        ),
    );
    object.insert("hgvsId".to_string(), optional_string(row, "hgvsId").into());
    object.insert(
        "variantDescription".to_string(),
        json!(string(row, "variantDescription")),
    );
    object.insert(
        "mostSevereConsequenceId".to_string(),
        json!(string(row, "mostSevereConsequenceId")),
    );

    // The credible sets and the pharmacogenomics of a variant come from other
    // tables in production. The tests do not seed those, so they are empty.
    object.insert("studyLocusIds".to_string(), Value::Array(Vec::new()));
    object.insert("pharmacogenomicsIds".to_string(), Value::Array(Vec::new()));

    Value::Object(object)
}

/// Returns the `variants` rows of the sample data, ready to be inserted into
/// ClickHouse.
pub fn variants() -> Vec<Value> {
    read_rows("output/variant/part-00000-00000.parquet")
        .iter()
        .map(variant_row)
        .collect()
}

/// Returns the search documents of the sample data, ready to be indexed into
/// OpenSearch.
pub fn search_targets() -> Vec<Value> { search_documents("view/search_target/00000000.parquet") }

/// Returns the search documents of the drugs of the sample data, ready to be
/// indexed into OpenSearch.
pub fn search_drugs() -> Vec<Value> { search_documents("view/search_drug/00000000.parquet") }

/// Returns the search documents of the diseases of the sample data, ready to be
/// indexed into OpenSearch.
pub fn search_diseases() -> Vec<Value> { search_documents("view/search_disease/00000000.parquet") }

/// Converts the rows of a search view into search documents.
///
/// The search views of all the entity types have the same shape, so they are
/// all read the same way.
fn search_documents(relative_path: &str) -> Vec<Value> {
    read_rows(relative_path)
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

/// Returns the identifier of the first drug of the sample data.
pub fn first_drug_id() -> String {
    let rows = read_rows("output/drug_molecule/00000000.parquet");
    string(rows.first().expect("sample data has no drugs"), "id")
}

/// Returns the name of the first drug of the sample data.
pub fn first_drug_name() -> String {
    let rows = read_rows("output/drug_molecule/00000000.parquet");
    string(rows.first().expect("sample data has no drugs"), "name")
}

/// Returns the identifier of the first disease of the sample data.
pub fn first_disease_id() -> String {
    let rows = read_rows("output/disease/00000000.parquet");
    string(rows.first().expect("sample data has no diseases"), "id")
}

/// Returns the identifier of the first variant of the sample data.
pub fn first_variant_id() -> String {
    let rows = read_rows("output/variant/part-00000-00000.parquet");
    string(
        rows.first().expect("sample data has no variants"),
        "variantId",
    )
}

/// Returns the identifier of the first study of the sample data.
pub fn first_study_id() -> String {
    let rows = read_rows("output/study/00000000.parquet");
    string(rows.first().expect("sample data has no studies"), "studyId")
}

/// Returns the identifier of the first clinical report of the sample data.
pub fn first_clinical_report_id() -> String {
    let rows = read_rows("output/clinical_report/00000000.parquet");
    string(
        rows.first().expect("sample data has no clinical reports"),
        "id",
    )
}

/// Returns the name of the first disease of the sample data.
pub fn first_disease_name() -> String {
    let rows = read_rows("output/disease/00000000.parquet");
    string(rows.first().expect("sample data has no diseases"), "name")
}
