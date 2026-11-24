//! Checks the Rust enums decoded from ClickHouse `Enum8`/`Enum16` columns against a database.
//!
//! `examples/check-clickhouse-enums.rs` reads each binding's type with
//! `DESCRIBE (SELECT <expr> FROM <table>)` and compares the value lists.

use strum::IntoEnumIterator;

#[cfg(feature = "product-ppp")]
use crate::entity::association_timeseries_ppp::AggregationType;
use crate::entity::{
    association::Datasource,
    baseline_expression::{BaselineExpressionDatasource, BaselineExpressionDatatype},
    clinical_report::{
        AgencyClass, ClinicalReportType, ClinicalStage, Origin, Provider, TrialLiteratureType,
        TrialOverallStatus, TrialPhase, TrialPrimaryPurpose, TrialStudyType,
    },
    credible_set::FineMappingMethod,
    disease_hpo::{HpoAspect, HpoEvidenceCode, HpoResource, HpoSex},
    drug::DrugType,
    drug_warning::DrugWarningType,
    enhancer_to_gene::{EnhancerToGeneDatasource, IntervalType},
    evidence::{
        Datatype, DirectionOnTarget, DirectionOnTrait, EuropePmcSection, TrialStopReasonCategory,
    },
    interaction::InteractionSourceDatabase,
    mechanisms_of_action::{MechanismActionType, MechanismCuration, MechanismTargetType},
    pharmacogenomics::{
        PharmacogenomicsCategory, PharmacogenomicsDatasource, PharmacogenomicsDatatype,
        PharmacogenomicsEvidenceLevel, VariantAnnotationEffectType,
    },
    study::{LdPopulation, StudyType},
    target::{
        ConstraintType, GoAspect, GoEvidenceCode, HomologyType, SafetyStudyType,
        TractabilityModality,
    },
    variant::{AlleleFrequencyPopulation, Chromosome, LofteePrediction, VariantEffectMethod},
};

/// A Rust enum and a ClickHouse expression it decodes.
pub struct EnumBinding {
    /// The Rust enum's name.
    pub rust: &'static str,
    /// Each variant's ClickHouse name and number.
    pub variants: fn() -> Vec<(&'static str, i16)>,
    /// The table the expression reads.
    pub table: &'static str,
    /// A SQL expression over `table` whose type is the enum.
    pub expr: &'static str,
}

macro_rules! binding {
    ($ty:ident, $table:literal, $expr:literal) => {
        EnumBinding {
            rust: stringify!($ty),
            variants: || {
                $ty::iter()
                    .map(|v| (<&'static str>::from(v), v as i16))
                    .collect()
            },
            table: $table,
            expr: $expr,
        }
    };
}

/// `Datasource`'s values. Platform's DDL keeps the PPP-only tail, which Platform data never holds
/// and which fails to decode if it does.
fn datasource_variants() -> Vec<(&'static str, i16)> {
    #[allow(unused_mut, reason = "extended on Platform only")]
    let mut values: Vec<(&'static str, i16)> = Datasource::iter()
        .map(|v| (<&'static str>::from(v), v as i16))
        .collect();
    #[cfg(not(feature = "product-ppp"))]
    values.extend([
        ("encore", 21),
        ("ot_crispr", 22),
        ("ot_crispr_validation", 23),
    ]);
    values
}

macro_rules! datasource_binding {
    ($table:literal) => {
        EnumBinding {
            rust: "Datasource",
            variants: datasource_variants,
            table: $table,
            expr: "datasourceId",
        }
    };
}

/// Every (Rust enum, ClickHouse column) pair the API decodes.
#[must_use]
pub fn bindings() -> Vec<EnumBinding> {
    vec![
        binding!(Chromosome, "variants", "chromosome"),
        binding!(Chromosome, "credible_sets", "chromosome"),
        binding!(Chromosome, "enhancer_to_gene", "chromosome"),
        binding!(Chromosome, "targets_by_region", "chromosome"),
        binding!(Chromosome, "study_credible_sets", "leadChromosome"),
        binding!(Chromosome, "variant_credible_sets", "leadChromosome"),
        binding!(Chromosome, "target_credible_sets", "leadChromosome"),
        binding!(
            Chromosome,
            "colocalisation",
            "tupleElement(arrayJoin(colocalisation), 'chromosome')"
        ),
        binding!(StudyType, "studies", "studyType"),
        binding!(StudyType, "credible_sets", "studyType"),
        binding!(StudyType, "variant_credible_sets", "studyType"),
        binding!(StudyType, "target_credible_sets", "studyType"),
        binding!(
            StudyType,
            "colocalisation",
            "tupleElement(arrayJoin(colocalisation), 'rightStudyType')"
        ),
        binding!(FineMappingMethod, "credible_sets", "fineMappingMethod"),
        binding!(
            FineMappingMethod,
            "study_credible_sets",
            "fineMappingMethod"
        ),
        binding!(
            FineMappingMethod,
            "variant_credible_sets",
            "fineMappingMethod"
        ),
        binding!(
            FineMappingMethod,
            "target_credible_sets",
            "fineMappingMethod"
        ),
        binding!(
            InteractionSourceDatabase,
            "interaction",
            "tupleElement(arrayJoin(interactions), 'sourceDatabase')"
        ),
        datasource_binding!("evidence"),
        datasource_binding!("evidence_by_disease_and_target"),
        datasource_binding!("evidence_by_variant"),
        binding!(Datatype, "evidence", "datatypeId"),
        datasource_binding!("associations_otf_disease"),
        datasource_binding!("associations_otf_target"),
        binding!(Datatype, "associations_otf_disease", "datatypeId"),
        binding!(Datatype, "associations_otf_target", "datatypeId"),
        #[cfg(feature = "product-ppp")]
        binding!(
            AggregationType,
            "association_time_series",
            "aggregationType"
        ),
        binding!(DirectionOnTarget, "evidence", "directionOnTarget"),
        binding!(DirectionOnTrait, "evidence", "directionOnTrait"),
        binding!(ClinicalStage, "evidence", "clinicalStage"),
        binding!(
            TrialStopReasonCategory,
            "evidence",
            "trialStopReasonCategories"
        ),
        binding!(
            EuropePmcSection,
            "evidence",
            "tupleElement(arrayJoin(textMiningSentences), 'section')"
        ),
        binding!(
            Chromosome,
            "targets",
            "tupleElement(genomicLocation, 'chromosome')"
        ),
        binding!(
            Chromosome,
            "targets",
            "tupleElement(canonicalTranscript, 'chromosome')"
        ),
        binding!(
            GoEvidenceCode,
            "targets",
            "tupleElement(arrayJoin(go), 'evidence')"
        ),
        binding!(GoAspect, "targets", "tupleElement(arrayJoin(go), 'aspect')"),
        binding!(
            ConstraintType,
            "targets",
            "tupleElement(arrayJoin(constraint), 'constraintType')"
        ),
        binding!(
            HomologyType,
            "targets",
            "tupleElement(arrayJoin(homologues), 'homologyType')"
        ),
        binding!(
            TractabilityModality,
            "targets",
            "tupleElement(arrayJoin(tractability), 'modality')"
        ),
        binding!(
            SafetyStudyType,
            "targets",
            "tupleElement(arrayJoin(tupleElement(arrayJoin(safetyLiabilities), 'studies')), 'type')"
        ),
        binding!(ClinicalStage, "clinical_report", "clinicalStage"),
        binding!(
            ClinicalStage,
            "clinical_indication_disease",
            "maxClinicalStage"
        ),
        binding!(ClinicalStage, "clinical_target", "maxClinicalStage"),
        binding!(ClinicalStage, "drug", "maximumClinicalStage"),
        binding!(Origin, "clinical_report", "origin"),
        binding!(ClinicalReportType, "clinical_report", "type"),
        binding!(Provider, "clinical_report", "provider"),
        binding!(TrialStudyType, "clinical_report", "trialStudyType"),
        binding!(TrialOverallStatus, "clinical_report", "trialOverallStatus"),
        binding!(TrialPhase, "clinical_report", "trialPhase"),
        binding!(
            TrialPrimaryPurpose,
            "clinical_report",
            "trialPrimaryPurpose"
        ),
        binding!(
            AgencyClass,
            "clinical_report",
            "tupleElement(trialSponsor, 'agencyClass')"
        ),
        binding!(
            TrialLiteratureType,
            "clinical_report",
            "tupleElement(arrayJoin(trialLiterature), 'type')"
        ),
        binding!(
            TrialStopReasonCategory,
            "clinical_report",
            "trialStopReasonCategories"
        ),
        binding!(DrugType, "drug", "drugType"),
        binding!(
            MechanismActionType,
            "mechanism_of_action",
            "uniqueActionTypes"
        ),
        binding!(
            MechanismTargetType,
            "mechanism_of_action",
            "uniqueTargetTypes"
        ),
        binding!(
            MechanismActionType,
            "mechanism_of_action",
            "tupleElement(arrayJoin(rows), 'actionType')"
        ),
        binding!(
            MechanismCuration,
            "mechanism_of_action",
            "tupleElement(arrayJoin(rows), 'curation')"
        ),
        binding!(
            DrugWarningType,
            "drug_warnings",
            "tupleElement(arrayJoin(drugWarnings), 'warningType')"
        ),
        binding!(
            HpoAspect,
            "disease_hpo",
            "tupleElement(arrayJoin(tupleElement(arrayJoin(phenotypes), 'evidence')), 'aspect')"
        ),
        binding!(
            HpoEvidenceCode,
            "disease_hpo",
            "tupleElement(arrayJoin(tupleElement(arrayJoin(phenotypes), 'evidence')), 'evidenceType')"
        ),
        binding!(
            HpoSex,
            "disease_hpo",
            "tupleElement(arrayJoin(tupleElement(arrayJoin(phenotypes), 'evidence')), 'sex')"
        ),
        binding!(
            HpoResource,
            "disease_hpo",
            "tupleElement(arrayJoin(tupleElement(arrayJoin(phenotypes), 'evidence')), 'resource')"
        ),
        binding!(
            LdPopulation,
            "studies",
            "tupleElement(arrayJoin(ldPopulationStructure), 'ldPopulation')"
        ),
        binding!(
            VariantEffectMethod,
            "variants",
            "tupleElement(arrayJoin(variantEffect), 'method')"
        ),
        binding!(
            AlleleFrequencyPopulation,
            "variants",
            "tupleElement(arrayJoin(alleleFrequencies), 'populationName')"
        ),
        binding!(
            LofteePrediction,
            "variants",
            "tupleElement(arrayJoin(transcriptConsequences), 'lofteePrediction')"
        ),
        binding!(
            BaselineExpressionDatasource,
            "baseline_expression",
            "datasourceId"
        ),
        binding!(
            BaselineExpressionDatatype,
            "baseline_expression",
            "datatypeId"
        ),
        binding!(
            PharmacogenomicsDatasource,
            "pharmacogenomics",
            "datasourceId"
        ),
        binding!(PharmacogenomicsDatatype, "pharmacogenomics", "datatypeId"),
        binding!(
            PharmacogenomicsEvidenceLevel,
            "pharmacogenomics",
            "evidenceLevel"
        ),
        binding!(PharmacogenomicsCategory, "pharmacogenomics", "pgxCategory"),
        binding!(
            VariantAnnotationEffectType,
            "pharmacogenomics",
            "tupleElement(arrayJoin(variantAnnotation), 'effectType')"
        ),
        binding!(IntervalType, "enhancer_to_gene", "intervalType"),
        binding!(EnhancerToGeneDatasource, "enhancer_to_gene", "datasourceId"),
    ]
}

/// The `(name, number)` values of a ClickHouse `Enum8`/`Enum16` type, unwrapping `Nullable`,
/// `LowCardinality` and `Array`.
///
/// # Errors
/// Returns an error if the type is not an enum or is malformed.
pub fn parse_enum_type(type_name: &str) -> Result<Vec<(String, i16)>, String> {
    let mut s = type_name.trim();
    let mut wrappers = 0;
    while let Some(inner) = ["Nullable(", "LowCardinality(", "Array("]
        .iter()
        .find_map(|w| s.strip_prefix(w))
    {
        s = inner;
        wrappers += 1;
    }
    let body = s
        .strip_prefix("Enum8(")
        .or_else(|| s.strip_prefix("Enum16("))
        .ok_or_else(|| format!("not an enum: {type_name}"))?;
    let mut chars = body.chars().peekable();
    let mut values = Vec::new();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next() != Some('\'') {
            return Err(format!("expected a quoted name in {type_name}"));
        }
        let mut name = String::new();
        loop {
            match chars.next() {
                Some('\\') => match chars.next() {
                    Some('n') => name.push('\n'),
                    Some('t') => name.push('\t'),
                    Some('0') => name.push('\0'),
                    Some(c) => name.push(c),
                    None => return Err(format!("unterminated name in {type_name}")),
                },
                Some('\'') => break,
                Some(c) => name.push(c),
                None => return Err(format!("unterminated name in {type_name}")),
            }
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next() != Some('=') {
            return Err(format!("expected '=' after '{name}' in {type_name}"));
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut number = String::new();
        while let Some(c) = chars.next_if(|c| *c == '-' || c.is_ascii_digit()) {
            number.push(c);
        }
        let number = number
            .parse::<i16>()
            .map_err(|e| format!("bad number for '{name}' in {type_name}: {e}"))?;
        values.push((name, number));
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        match chars.next() {
            Some(',') => {}
            Some(')') => break,
            _ => return Err(format!("expected ',' or ')' in {type_name}")),
        }
    }
    let rest: String = chars.filter(|c| !c.is_whitespace()).collect();
    if rest != ")".repeat(wrappers) {
        return Err(format!("unexpected trailing '{rest}' in {type_name}"));
    }
    Ok(values)
}

/// Differences between a Rust enum's values and a ClickHouse enum's values.
#[must_use]
pub fn mismatches(rust: &[(&str, i16)], clickhouse: &[(String, i16)]) -> Vec<String> {
    let mut out = Vec::new();
    for (name, number) in clickhouse {
        match rust.iter().find(|(n, _)| n == name) {
            None => out.push(format!("missing value '{name}' = {number}")),
            Some((_, r)) if r != number => out.push(format!(
                "number differs for '{name}': rust {r}, clickhouse {number}"
            )),
            Some(_) => {}
        }
    }
    for (name, number) in rust {
        if !clickhouse.iter().any(|(n, _)| n == name) {
            out.push(format!("extra value '{name}' = {number}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[(&str, i16)]) -> Vec<(String, i16)> {
        v.iter().map(|(n, i)| ((*n).to_string(), *i)).collect()
    }

    #[test]
    fn parses_enum8() {
        assert_eq!(
            parse_enum_type("Enum8('a' = 1, 'b' = 2)"),
            Ok(owned(&[("a", 1), ("b", 2)]))
        );
    }

    #[test]
    fn unwraps_nullable_and_reads_negative_numbers() {
        assert_eq!(
            parse_enum_type("Nullable(Enum8('-1' = -1, '0' = 0, '1' = 1))"),
            Ok(owned(&[("-1", -1), ("0", 0), ("1", 1)]))
        );
    }

    #[test]
    fn unwraps_array_and_low_cardinality_and_unescapes_quotes() {
        assert_eq!(
            parse_enum_type("Array(LowCardinality(Enum8('it\\'s' = 1)))"),
            Ok(owned(&[("it's", 1)]))
        );
    }

    #[test]
    fn rejects_non_enum_types() {
        assert!(parse_enum_type("String").is_err());
    }

    #[test]
    fn reports_missing_extra_and_renumbered_values() {
        let found = mismatches(
            &[("a", 1), ("b", 2), ("x", 9)],
            &owned(&[("a", 1), ("b", 3), ("c", 4)]),
        );
        assert_eq!(
            found,
            [
                "number differs for 'b': rust 2, clickhouse 3",
                "missing value 'c' = 4",
                "extra value 'x' = 9",
            ]
        );
    }

    #[test]
    fn bindings_list_every_variant() {
        for b in bindings() {
            assert!(!(b.variants)().is_empty(), "{}", b.rust);
        }
    }
}
