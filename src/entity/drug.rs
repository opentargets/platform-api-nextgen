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
pub struct Drug {
    id: String,
    name: String,
    synonyms: Vec<DrugLabelAndSource>,
    tradeNames: Vec<DrugLabelAndSource>,
    drugType: String,
    crossReferences: Vec<DrugReferences>,
    parentId: Option<String>,
    maximumClinicalStage: String,
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
