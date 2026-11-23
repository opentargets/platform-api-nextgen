//! The GraphQL documents the frontend sends to this API.
//!
//! The files in `tests/gql` are verbatim copies of the documents in
//! `platform-webapp`, so that the integration tests exercise the queries the
//! frontend actually runs, instead of copies that can drift away from them.
//!
//! Every document here resolves from the `targets` table alone, so the tests
//! can run them without seeding the other tables of the schema. A document
//! that needs another table would fail with a missing-table error.

use std::path::PathBuf;

use super::sample_data;

/// The value of a variable the frontend sends.
#[derive(Debug, Clone, Copy)]
pub enum Variable {
    /// A number.
    Number(i64),
    /// A list of sort statements, which the API rejects when it is not empty.
    EmptySort,
    /// An empty string.
    Text,
}

/// The kind of GraphQL document a vendored file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A complete query, which the API can execute as it is.
    Operation,
    /// A fragment, which is not executable on its own and has to be wrapped in
    /// a query that spreads it.
    Fragment,
    /// A query that fetches a list of entities, which the API can execute as it
    /// is, but which takes a list of identifiers instead of a single one.
    RootList,
}

/// The field of the response a document puts its entity under.
///
/// The queries of the frontend fetch one entity, and each one of them is a
/// field of the response, named after the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// The `target` field.
    Target,
    /// The `disease` field.
    Disease,
    /// The `drug` field.
    Drug,
    /// The `diseases` field, which holds a list of entities.
    Diseases,
    /// The `studies` field, which holds a list of entities.
    Studies,
    /// The `clinicalReport` field.
    ClinicalReport,
    /// The `clinicalReports` field, which holds a list of entities.
    ClinicalReports,
}

impl Root {
    /// The name of the field in the response.
    fn field(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Disease => "disease",
            Self::Drug => "drug",
            Self::Diseases => "diseases",
            Self::Studies => "studies",
            Self::ClinicalReport => "clinicalReport",
            Self::ClinicalReports => "clinicalReports",
        }
    }

    /// The path of the entity in the response.
    ///
    /// A list query puts the entity in the rows of a page.
    #[must_use]
    pub fn path(self) -> String {
        match self {
            Self::Diseases => "/data/diseases/rows/0".to_string(),
            Self::Studies => "/data/studies/rows/0".to_string(),
            Self::ClinicalReports => "/data/clinicalReports/0".to_string(),
            _ => format!("/data/{}", self.field()),
        }
    }
}

/// The root query a fragment is spread into.
///
/// A fragment is defined on a type, and the wrapper that makes it executable
/// has to query that type. Every type the frontend writes fragments for has a
/// root query of its own, taking the identifier of the entity as its
/// argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    /// The `target` query, which takes an Ensembl ID.
    Target,
    /// The `disease` query, which takes an EFO ID.
    Disease,
    /// The `drug` query, which takes a ChEMBL ID.
    Drug,
    /// The `study` query, which takes a study ID.
    Study,
    /// The `clinicalReport` query, which takes a clinical report ID.
    ClinicalReport,
}

impl Parent {
    /// The name of the root query.
    fn query(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Disease => "disease",
            Self::Drug => "drug",
            Self::Study => "study",
            Self::ClinicalReport => "clinicalReport",
        }
    }

    /// The name of the argument the root query takes the identifier in.
    fn argument(self) -> &'static str {
        match self {
            Self::Target => "ensemblId",
            Self::Disease => "efoId",
            Self::Drug => "chemblId",
            Self::Study => "studyId",
            Self::ClinicalReport => "clinicalReportId",
        }
    }

    /// The GraphQL type the root query returns.
    fn type_name(self) -> &'static str {
        match self {
            Self::Target => "Target",
            Self::Disease => "Disease",
            Self::Drug => "Drug",
            Self::Study => "Study",
            Self::ClinicalReport => "ClinicalReport",
        }
    }

    /// The parent of the fragment defined on the given type.
    ///
    /// The frontend writes fragments for the three types the API can fetch by
    /// identifier. A fragment on any other type has no root query to be
    /// spread into.
    fn of_type(type_name: &str) -> Option<Self> {
        match type_name {
            "Target" => Some(Self::Target),
            "Disease" => Some(Self::Disease),
            "Drug" => Some(Self::Drug),
            "Study" => Some(Self::Study),
            "ClinicalReport" => Some(Self::ClinicalReport),
            _ => None,
        }
    }
}

/// A document vendored from the frontend.
#[derive(Debug, Clone, Copy)]
pub struct FrontendQuery {
    /// The name of the file in `tests/gql`, without the extension.
    pub name: &'static str,
    /// The path of the file in `platform-webapp` it was copied from.
    pub source: &'static str,
    /// Whether the file holds an operation or a fragment.
    pub kind: Kind,
    /// The root query a fragment is spread into.
    ///
    /// Only meaningful for a fragment: the type the fragment is defined on
    /// decides which query can fetch it.
    pub parent: Option<Parent>,
    /// The name of the variable the document declares for the target
    /// identifier. The frontend is not consistent about it: most documents use
    /// `ensemblId`, but the target page uses `ensgId`.
    pub variable: &'static str,
    /// The other variables the document declares, with the values to send.
    ///
    /// The frontend sends the size of the page, the sort and the search term
    /// along with the identifier, so a document needs more than one variable.
    pub variables: &'static [(&'static str, Variable)],
    /// The field of the response the document puts its entity under.
    pub root: Root,
    /// The fields the document selects that the sample data has values for.
    ///
    /// The sample data has no chemical probes, pathways or safety liabilities
    /// for its targets, so those fields cannot be checked for a value. The
    /// other fields can, and have to hold one.
    pub expected: &'static [&'static str],
}

/// The documents vendored from the frontend.
///
/// The source path is recorded here, so that a document can be traced back to
/// the file it was copied from without having to guess from the name.
pub const QUERIES: &[FrontendQuery] = &[
    FrontendQuery {
        name: "AssociationsToolkit_components_Table_RowInteractors_InteractorsQuery",
        source: "apps/platform/src/components/AssociationsToolkit/components/Table/RowInteractors/InteractorsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensgId",
        variables: &[],
        root: Root::Target,
        expected: &["approvedName", "approvedSymbol", "id"],
    },
    FrontendQuery {
        name: "DiseasePage_DiseaseAssociations_DiseaseAssociationsQuery",
        source: "apps/platform/src/pages/DiseasePage/DiseaseAssociations/DiseaseAssociationsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "id",
        variables: &[],
        root: Root::Disease,
        expected: &["id", "name"],
    },
    FrontendQuery {
        name: "DiseasePage_DiseasePage",
        source: "apps/platform/src/pages/DiseasePage/DiseasePage.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "efoId",
        variables: &[],
        root: Root::Disease,
        expected: &["dbXRefs", "id", "name"],
    },
    FrontendQuery {
        name: "DiseasePage_ProfileHeader",
        source: "apps/platform/src/pages/DiseasePage/ProfileHeader.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Disease),
        variable: "None",
        variables: &[],
        root: Root::Disease,
        expected: &["description", "synonyms"],
    },
    FrontendQuery {
        name: "DrugPage_DrugPage",
        source: "apps/platform/src/pages/DrugPage/DrugPage.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "chemblId",
        variables: &[],
        root: Root::Drug,
        expected: &["crossReferences", "id", "name"],
    },
    FrontendQuery {
        name: "DrugPage_ProfileHeader",
        source: "apps/platform/src/pages/DrugPage/ProfileHeader.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &[
            "description",
            "drugType",
            "maximumClinicalStage",
            "synonyms",
        ],
    },
    FrontendQuery {
        name: "GeneEnrichmentAnalysis_api_DiaseaseAssociationsQuery",
        source: "apps/platform/src/components/GeneEnrichmentAnalysis/api/DiaseaseAssociationsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "efoId",
        variables: &[],
        root: Root::Disease,
        expected: &["id", "name"],
    },
    FrontendQuery {
        name: "TargetPage_TargetAssociations_TargetAssociationsQuery",
        source: "apps/platform/src/pages/TargetPage/TargetAssociations/TargetAssociationsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "id",
        variables: &[],
        root: Root::Target,
        expected: &["approvedSymbol", "id"],
    },
    FrontendQuery {
        name: "TargetPage_TargetPage",
        source: "apps/platform/src/pages/TargetPage/TargetPage.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensgId",
        variables: &[],
        root: Root::Target,
        expected: &[
            "approvedName",
            "approvedSymbol",
            "dbXrefs",
            "id",
            "proteinIds",
        ],
    },
    FrontendQuery {
        name: "TargetPage_TargetProfileHeader",
        source: "apps/platform/src/pages/TargetPage/TargetProfileHeader.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["canonicalTranscript", "genomicLocation", "synonyms"],
    },
    FrontendQuery {
        name: "components_ClinicalReports_ClinicalRecordsQuery",
        source: "packages/ui/src/components/ClinicalReports/ClinicalRecordsQuery.gql",
        kind: Kind::RootList,
        parent: Some(Parent::ClinicalReport),
        variable: "clinicalReportsIds",
        variables: &[],
        root: Root::ClinicalReports,
        expected: &["clinicalStage", "id", "source", "title", "type"],
    },
    FrontendQuery {
        name: "components_ClinicalReports_RecordDetailQuery",
        source: "packages/ui/src/components/ClinicalReports/RecordDetailQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::ClinicalReport),
        variable: "clinicalReportId",
        variables: &[],
        root: Root::ClinicalReport,
        expected: &[
            "clinicalStage",
            "id",
            "phaseFromSource",
            "source",
            "title",
            "trialSponsor",
            "type",
            "url",
        ],
    },
    FrontendQuery {
        name: "disease_Bibliography_BibliographyQuery",
        source: "packages/sections/src/disease/Bibliography/BibliographyQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "id",
        variables: &[],
        root: Root::Disease,
        expected: &["id", "name"],
    },
    FrontendQuery {
        name: "disease_Bibliography_BibliographySummaryFragment",
        source: "packages/sections/src/disease/Bibliography/BibliographySummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Disease),
        variable: "None",
        variables: &[],
        root: Root::Disease,
        expected: &["id"],
    },
    FrontendQuery {
        name: "disease_Drugs_DrugsQuery",
        source: "packages/sections/src/disease/Drugs/DrugsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "efoId",
        variables: &[],
        root: Root::Disease,
        expected: &["name"],
    },
    FrontendQuery {
        name: "disease_Drugs_DrugsSummaryFragment",
        source: "packages/sections/src/disease/Drugs/DrugsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Disease),
        variable: "None",
        variables: &[],
        root: Root::Disease,
        expected: &["id"],
    },
    FrontendQuery {
        name: "disease_GWASStudies_GWASStudiesQuery",
        source: "packages/sections/src/disease/GWASStudies/GWASStudiesQuery.gql",
        kind: Kind::RootList,
        parent: Some(Parent::Study),
        variable: "diseaseIds",
        variables: &[
            ("size", Variable::Number(10)),
            ("index", Variable::Number(0)),
            ("sort", Variable::EmptySort),
            ("search", Variable::Text),
        ],
        root: Root::Studies,
        expected: &[
            "credibleSets",
            "id",
            "ldPopulationStructure",
            "nSamples",
            "projectId",
            "publicationDate",
            "publicationFirstAuthor",
            "publicationJournal",
            "pubmedId",
            "traitFromSource",
        ],
    },
    FrontendQuery {
        name: "disease_Ontology_OntologyAncestorsQuery",
        source: "packages/sections/src/disease/Ontology/OntologyAncestorsQuery.gql",
        kind: Kind::RootList,
        parent: Some(Parent::Disease),
        variable: "ids",
        variables: &[("size", Variable::Number(10))],
        root: Root::Diseases,
        expected: &["id", "isTherapeuticArea", "name"],
    },
    FrontendQuery {
        name: "disease_Ontology_OntologyQuery",
        source: "packages/sections/src/disease/Ontology/OntologyQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "efoId",
        variables: &[],
        root: Root::Disease,
        expected: &["ancestors", "id", "isTherapeuticArea", "name"],
    },
    FrontendQuery {
        name: "disease_Ontology_OntologySummaryFragment",
        source: "packages/sections/src/disease/Ontology/OntologySummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Disease),
        variable: "None",
        variables: &[],
        root: Root::Disease,
        expected: &["id", "isTherapeuticArea", "name"],
    },
    FrontendQuery {
        name: "disease_Phenotypes_PhenotypesQuery",
        source: "packages/sections/src/disease/Phenotypes/PhenotypesQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Disease),
        variable: "efoId",
        variables: &[],
        root: Root::Disease,
        expected: &["id"],
    },
    FrontendQuery {
        name: "disease_Phenotypes_PhenotypesSummaryFragment",
        source: "packages/sections/src/disease/Phenotypes/PhenotypesSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Disease),
        variable: "None",
        variables: &[],
        root: Root::Disease,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_AdverseEvents_AdverseEventsQuery",
        source: "packages/sections/src/drug/AdverseEvents/AdverseEventsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "chemblId",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_AdverseEvents_AdverseEventsSummaryFragment",
        source: "packages/sections/src/drug/AdverseEvents/AdverseEventsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_Bibliography_BibliographyQuery",
        source: "packages/sections/src/drug/Bibliography/BibliographyQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "id",
        variables: &[],
        root: Root::Drug,
        expected: &["id", "name"],
    },
    FrontendQuery {
        name: "drug_Bibliography_BibliographySummaryFragment",
        source: "packages/sections/src/drug/Bibliography/BibliographySummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_DrugWarnings_DrugWarningsQuery",
        source: "packages/sections/src/drug/DrugWarnings/DrugWarningsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "chemblId",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_DrugWarnings_DrugWarningsSummaryFragment",
        source: "packages/sections/src/drug/DrugWarnings/DrugWarningsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_Indications_IndicationsQuery",
        source: "packages/sections/src/drug/Indications/IndicationsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "chemblId",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_Indications_IndicationsSummaryFragment",
        source: "packages/sections/src/drug/Indications/IndicationsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_MechanismsOfAction_MechanismsOfActionQuery",
        source: "packages/sections/src/drug/MechanismsOfAction/MechanismsOfActionQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "chemblId",
        variables: &[],
        root: Root::Drug,
        expected: &["id"],
    },
    FrontendQuery {
        name: "drug_MechanismsOfAction_MechanismsOfActionSummaryFragment",
        source: "packages/sections/src/drug/MechanismsOfAction/MechanismsOfActionSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &[],
    },
    FrontendQuery {
        name: "drug_Pharmacogenomics_Pharmacogenomics",
        source: "packages/sections/src/drug/Pharmacogenomics/Pharmacogenomics.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Drug),
        variable: "chemblId",
        variables: &[
            ("index", Variable::Number(0)),
            ("size", Variable::Number(10)),
        ],
        root: Root::Drug,
        expected: &["id", "pharmacogenomics"],
    },
    FrontendQuery {
        name: "drug_Pharmacogenomics_PharmacogenomicsSummary",
        source: "packages/sections/src/drug/Pharmacogenomics/PharmacogenomicsSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Drug),
        variable: "None",
        variables: &[],
        root: Root::Drug,
        expected: &["pharmacogenomics"],
    },
    FrontendQuery {
        name: "target_BaselineExpression_ExpressionQuery",
        source: "packages/sections/src/target/BaselineExpression/ExpressionQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_BaselineExpression_ExpressionSummary",
        source: "packages/sections/src/target/BaselineExpression/ExpressionSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Bibliography_SimilarEntities",
        source: "packages/sections/src/target/Bibliography/SimilarEntities.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "id",
        variables: &[],
        root: Root::Target,
        expected: &["approvedName", "id"],
    },
    FrontendQuery {
        name: "target_Bibliography_SimilarEntitiesSummary",
        source: "packages/sections/src/target/Bibliography/SimilarEntitiesSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_CancerHallmarks_Hallmarks",
        source: "packages/sections/src/target/CancerHallmarks/Hallmarks.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["hallmarks", "id"],
    },
    FrontendQuery {
        name: "target_CancerHallmarks_HallmarksSummaryFragment",
        source: "packages/sections/src/target/CancerHallmarks/HallmarksSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["hallmarks"],
    },
    FrontendQuery {
        name: "target_ChemicalProbes_ChemicalProbes",
        source: "packages/sections/src/target/ChemicalProbes/ChemicalProbes.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_ChemicalProbes_ProbesSummaryFragment",
        source: "packages/sections/src/target/ChemicalProbes/ProbesSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &[],
    },
    FrontendQuery {
        name: "target_ComparativeGenomics_CompGenomics",
        source: "packages/sections/src/target/ComparativeGenomics/CompGenomics.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["homologues", "id"],
    },
    FrontendQuery {
        name: "target_ComparativeGenomics_CompGenomicsSummaryFragment",
        source: "packages/sections/src/target/ComparativeGenomics/CompGenomicsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["homologues"],
    },
    FrontendQuery {
        name: "target_DepMap_Depmap",
        source: "packages/sections/src/target/DepMap/Depmap.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_DepMap_DepmapSummaryFragment",
        source: "packages/sections/src/target/DepMap/DepmapSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Drugs_DrugsQuery",
        source: "packages/sections/src/target/Drugs/DrugsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Drugs_DrugsSummaryFragment",
        source: "packages/sections/src/target/Drugs/DrugsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_GeneOntology_GeneOntology",
        source: "packages/sections/src/target/GeneOntology/GeneOntology.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_GeneOntology_GeneOntologySummary",
        source: "packages/sections/src/target/GeneOntology/GeneOntologySummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["geneOntology", "id"],
    },
    FrontendQuery {
        name: "target_GeneticConstraint_GeneticConstraint",
        source: "packages/sections/src/target/GeneticConstraint/GeneticConstraint.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["geneticConstraint", "id"],
    },
    FrontendQuery {
        name: "target_GeneticConstraint_GeneticConstraintFragment",
        source: "packages/sections/src/target/GeneticConstraint/GeneticConstraintFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["geneticConstraint"],
    },
    FrontendQuery {
        name: "target_MolecularInteractions_InteractionsQuery",
        source: "packages/sections/src/target/MolecularInteractions/InteractionsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensgId",
        variables: &[],
        root: Root::Target,
        expected: &["approvedName", "approvedSymbol", "id"],
    },
    FrontendQuery {
        name: "target_MolecularInteractions_InteractionsStats",
        source: "packages/sections/src/target/MolecularInteractions/InteractionsStats.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensgId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_MolecularInteractions_InteractionsStringQuery",
        source: "packages/sections/src/target/MolecularInteractions/InteractionsStringQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensgId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_MolecularInteractions_InteractionsSummary",
        source: "packages/sections/src/target/MolecularInteractions/InteractionsSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_MolecularStructure_MolecularStructure",
        source: "packages/sections/src/target/MolecularStructure/MolecularStructure.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["proteinIds"],
    },
    FrontendQuery {
        name: "target_MolecularStructure_summaryQuery",
        source: "packages/sections/src/target/MolecularStructure/summaryQuery.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["proteinIds"],
    },
    FrontendQuery {
        name: "target_MousePhenotypes_MousePhenotypes",
        source: "packages/sections/src/target/MousePhenotypes/MousePhenotypes.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_MousePhenotypes_MousePhenotypesSummary",
        source: "packages/sections/src/target/MousePhenotypes/MousePhenotypesSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Pathways_Pathways",
        source: "packages/sections/src/target/Pathways/Pathways.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["approvedSymbol", "id"],
    },
    FrontendQuery {
        name: "target_Pathways_PathwaysSummary",
        source: "packages/sections/src/target/Pathways/PathwaysSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &[],
    },
    FrontendQuery {
        name: "target_Pharmacogenomics_Pharmacogenomics",
        source: "packages/sections/src/target/Pharmacogenomics/Pharmacogenomics.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Pharmacogenomics_PharmacogenomicsSummary",
        source: "packages/sections/src/target/Pharmacogenomics/PharmacogenomicsSummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_QTLCredibleSets_QTLCredibleSetsQuery",
        source: "packages/sections/src/target/QTLCredibleSets/QTLCredibleSetsQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["approvedSymbol", "id"],
    },
    FrontendQuery {
        name: "target_QTLCredibleSets_QTLCredibleSetsSummaryFragment",
        source: "packages/sections/src/target/QTLCredibleSets/QTLCredibleSetsSummaryFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Safety_Safety",
        source: "packages/sections/src/target/Safety/Safety.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id"],
    },
    FrontendQuery {
        name: "target_Safety_summaryQuery",
        source: "packages/sections/src/target/Safety/summaryQuery.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &[],
    },
    FrontendQuery {
        name: "target_SubcellularLocation_SubcellularLocation",
        source: "packages/sections/src/target/SubcellularLocation/SubcellularLocation.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id", "proteinIds", "subcellularLocations"],
    },
    FrontendQuery {
        name: "target_SubcellularLocation_SubcellularLocationFragment",
        source: "packages/sections/src/target/SubcellularLocation/SubcellularLocationFragment.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["subcellularLocations"],
    },
    FrontendQuery {
        name: "target_Tractability_TractabilityQuery",
        source: "packages/sections/src/target/Tractability/TractabilityQuery.gql",
        kind: Kind::Operation,
        parent: Some(Parent::Target),
        variable: "ensemblId",
        variables: &[],
        root: Root::Target,
        expected: &["id", "tractability"],
    },
    FrontendQuery {
        name: "target_Tractability_TractabilitySummary",
        source: "packages/sections/src/target/Tractability/TractabilitySummary.gql",
        kind: Kind::Fragment,
        parent: Some(Parent::Target),
        variable: "None",
        variables: &[],
        root: Root::Target,
        expected: &["id", "tractability"],
    },
];

/// The directory the vendored documents live in.
fn gql_dir() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/gql") }

/// Returns the description of the vendored document with the given name.
#[must_use]
pub fn query(name: &str) -> &'static FrontendQuery {
    QUERIES
        .iter()
        .find(|query| query.name == name)
        .unwrap_or_else(|| panic!("no frontend query named {name}"))
}

/// Reads the vendored document with the given name.
///
/// The file is read verbatim: the tests are meant to run the query the
/// frontend sends, not a reformatted version of it.
#[must_use]
pub fn document(name: &str) -> String {
    let path = gql_dir().join(format!("{name}.gql"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read the frontend query {}: {e}", path.display()))
}

/// Returns the document to send to the API.
///
/// Operations and list queries are sent as they are. Fragments are not
/// executable on their own, so they are wrapped in a query that spreads them
/// into the root query of their type. The wrapper declares the variable the
/// fragment is spread into, with the name the frontend uses for it, so that
/// the request looks like the one the frontend sends.
#[must_use]
pub fn executable(query: &FrontendQuery) -> String {
    let document = document(query.name);
    match query.kind {
        Kind::Operation | Kind::RootList => document,
        Kind::Fragment => {
            let fragment_name = fragment_name(&document);
            let parent = query
                .parent
                .unwrap_or_else(|| panic!("the fragment {} has no parent", query.name));
            format!(
                "{document}\n\
                 query {fragment_name}Wrapper(${variable}: String!) {{\n  \
                 {query}({argument}: ${variable}) {{\n    ...{fragment_name}\n  \
                 }}\n}}",
                query = parent.query(),
                argument = parent.argument(),
                variable = query.variable,
            )
        }
    }
}

/// Extracts the name of the fragment a document defines.
///
/// The frontend names its fragments like `fragment SafetySummaryFragment on
/// Target { ... }`.
fn fragment_name(document: &str) -> String {
    document
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("fragment ")?;
            let name = rest.split_whitespace().next()?;
            name.chars()
                .all(|c| c.is_alphanumeric() || c == '_')
                .then(|| name.to_string())
        })
        .expect("the document defines a fragment")
}

/// Extracts the type a fragment is defined on.
///
/// The frontend names its fragments like `fragment SafetySummaryFragment on
/// Target { ... }`.
fn fragment_type(document: &str) -> Option<&str> {
    document.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("fragment ")?;
        let mut parts = rest.split_whitespace();
        let _name = parts.next()?;
        let _on = parts.next()?;
        let type_name = parts.next()?;
        Some(type_name)
    })
}

/// Returns the variables to send with the given document.
///
/// The frontend binds the identifier to a variable whose name differs between
/// documents, so the variable has to be named the way the document declares
/// it. A list query takes a list of identifiers, the others a single one.
#[must_use]
pub fn variables(query: &FrontendQuery) -> serde_json::Value {
    let id = match query.parent {
        Some(Parent::Drug) => sample_data::first_drug_id(),
        Some(Parent::Disease) => sample_data::first_disease_id(),
        Some(Parent::Study) => sample_data::first_study_id(),
        Some(Parent::ClinicalReport) => sample_data::first_clinical_report_id(),
        Some(Parent::Target) | None => sample_data::first_target_id(),
    };
    // A list of studies is selected by the diseases the studies are about, so
    // the identifier of the disease is sent.
    let id = if query.variable == "diseaseIds" {
        sample_data::first_disease_id()
    } else {
        id
    };
    let mut variables = serde_json::Map::new();
    variables.insert(
        query.variable.to_string(),
        match query.kind {
            Kind::RootList => serde_json::json!([id]),
            _ => serde_json::json!(id),
        },
    );
    for (name, value) in query.variables {
        variables.insert(
            (*name).to_string(),
            match value {
                Variable::Number(number) => serde_json::json!(number),
                Variable::EmptySort => serde_json::json!([]),
                Variable::Text => serde_json::json!(""),
            },
        );
    }
    serde_json::Value::Object(variables)
}

/// Checks that the manifest agrees with the vendored documents.
///
/// The parent of a fragment has to be the type the fragment is defined on,
/// otherwise the wrapper would spread it into a query that returns another
/// type, and the API would reject the request.
pub fn check_manifest() {
    for query in QUERIES {
        let document = document(query.name);
        match query.kind {
            Kind::Operation | Kind::RootList => assert!(
                !document.trim_start().starts_with("fragment "),
                "the document {} is a fragment, but the manifest calls it an operation",
                query.name
            ),
            Kind::Fragment => {
                let type_name = fragment_type(&document)
                    .unwrap_or_else(|| panic!("the document {} defines no fragment", query.name));
                let parent = query
                    .parent
                    .unwrap_or_else(|| panic!("the fragment {} has no parent", query.name));
                assert_eq!(
                    parent.type_name(),
                    type_name,
                    "the manifest gives the fragment {} the parent {}, but it is defined on {type_name}",
                    query.name,
                    parent.type_name()
                );
                assert!(
                    Parent::of_type(type_name).is_some(),
                    "the fragment {} is defined on {type_name}, which the API has no root query for",
                    query.name
                );
            }
        }
    }
}
