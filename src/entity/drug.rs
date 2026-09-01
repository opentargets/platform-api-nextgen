use async_graphql::SimpleObject;
use serde::Deserialize;

// ---- models ----

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugReferences {
    source: String,
    ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugLabelAndSource {
    label: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Drug {
    id: String,
    name: String,
    synonyms: Vec<DrugLabelAndSource>,
    trade_names: Vec<DrugLabelAndSource>,
    #[allow(clippy::struct_field_names)]
    drug_type: String,
    cross_references: Vec<DrugReferences>,
    parent_id: Option<String>,
    maximum_clinical_stage: String,
    description: Option<String>,
    molblock: Option<String>,
}

// object Drug {
//   implicit val getResult: GetResult[Drug] = GetResult(fromPositionedResult[Drug])
//   implicit val DrugXRefImpW: OFormat[DrugReferences] = Json.format[DrugReferences]
//   implicit val drugLabelAndSourceImpF: OFormat[DrugLabelAndSource] =
// Json.format[DrugLabelAndSource]   implicit val drugImplicitR: Reads[Drug] = Json.reads[Drug]
//   implicit val drugImplicitW: OWrites[Drug] = Json.writes[Drug]
// }
