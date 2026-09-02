package models.entities

import utils.OTLogging
import play.api.libs.json._
import slick.jdbc.GetResult
import utils.db.DbJsonParser.fromPositionedResult

pub struct DrugWarningReference{
    id: String,
    source: String,
    url: String
}

pub struct DrugWarning{
    toxicityClass: Option<String>,
    chemblIds: Vec<String>,
    country: Option<String>,
    description: Option<String>,
    id: Option<i64>,
    references: Vec<DrugWarningReference>,
    warningType: String,
    year: Option<i32>,
    efoTerm: Option<String>,
    efoId: Option<String>,
    efoIdForWarningClass: Option<String>,
}

pub struct DrugWarnings{
    chemblId: String,
    drugWarnings: Vec<DrugWarning>,
}

object DrugWarning extends OTLogging {
  implicit val getDrugWarningsFromDB: GetResult[DrugWarnings] =
    GetResult(fromPositionedResult[DrugWarnings])
  implicit val drugWarningsImpF: OFormat[DrugWarnings] = Json.format[models.entities.DrugWarnings]
  implicit val drugWarningsReferenceImpF: OFormat[models.entities.DrugWarningReference] =
    Json.format[models.entities.DrugWarningReference]
  implicit val drugWarningImpF: OFormat[models.entities.DrugWarning] =
    Json.format[models.entities.DrugWarning]
}
