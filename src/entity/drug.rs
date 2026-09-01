package models.entities

import play.api.libs.json._
import slick.jdbc.GetResult
import utils.db.DbJsonParser.fromPositionedResult

pub struct DrugReferences{
    source: String,
    ids: Vec<String>,
}

pub struct DrugLabelAndSource{
    label: String,
    source: String,
}

pub struct Drug{
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

object Drug {
  implicit val getResult: GetResult[Drug] = GetResult(fromPositionedResult[Drug])
  implicit val DrugXRefImpW: OFormat[DrugReferences] = Json.format[DrugReferences]
  implicit val drugLabelAndSourceImpF: OFormat[DrugLabelAndSource] = Json.format[DrugLabelAndSource]
  implicit val drugImplicitR: Reads[Drug] = Json.reads[Drug]
  implicit val drugImplicitW: OWrites[Drug] = Json.writes[Drug]
}
