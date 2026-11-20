//! One SQL-paged view per evidence datasource, each shaped like that datasource's table on the
//! evidence page.
//!
//! A datasource module doesn't know whether it is resolved from a disease or a target: its key
//! always carries an [`EvidenceScope`] with both id lists, and the entity fills in its own id.
//! `Disease` and `Target` each expose one `evidenceViews` field returning [`EvidenceViews`], which
//! holds the scope and has one field per datasource.

use async_graphql::{Context, Error, Object, Result};

use crate::query::{
    paginate::{Page, Paged, page_complexity},
    sort::{Sort, UniqueSortKeys},
    sql::{Column, Filter, Op, Value},
};

pub mod clingen;
pub mod clinical_precedence;
pub mod crispr;
pub mod eva;
pub mod expression_atlas;
pub mod gene2phenotype;
pub mod gene_burden;
pub mod genomics_england_panel_app;
pub mod gwas_credible_sets;
pub mod impc;
pub mod orphanet;
pub mod reactome;
pub mod uniprot_literature;
pub mod uniprot_variants;

/// Evidence ids for the scope's targets and diseases, read from the index table so `evidence` is
/// looked up by key. Binds: datasource, disease ids (expanded to their ontology descendants),
/// target ids.
const INDIRECT_EVIDENCE_IDS: &str = "SELECT id FROM evidence_by_disease_and_target \
     WHERE datasourceId = ? \
     AND diseaseId IN (SELECT arrayJoin(arrayConcat(descendants, [id])) FROM disease WHERE id IN ?) \
     AND targetId IN ?";
/// As [`INDIRECT_EVIDENCE_IDS`], for the given diseases only.
const DIRECT_EVIDENCE_IDS: &str = "SELECT id FROM evidence_by_disease_and_target \
     WHERE datasourceId = ? AND diseaseId IN ? AND targetId IN ?";

/// The diseases plus their ontology descendants. Bind: disease ids.
const DISEASES_AND_DESCENDANTS: &str =
    "SELECT arrayJoin(arrayConcat(descendants, [id])) FROM disease WHERE id IN ?";

/// The disease/target pairs whose evidence a view shows.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EvidenceScope {
    /// Sorted and deduplicated, so equal sets of ids give equal loader keys.
    efo_ids: Vec<String>,
    /// Sorted and deduplicated, as `efo_ids`.
    ensembl_ids: Vec<String>,
    /// Whether to include evidence for the diseases' ontology descendants.
    enable_indirect: bool,
}

impl EvidenceScope {
    #[must_use]
    pub fn new(
        mut efo_ids: Vec<String>,
        mut ensembl_ids: Vec<String>,
        enable_indirect: bool,
    ) -> Self {
        efo_ids.sort_unstable();
        efo_ids.dedup();
        ensembl_ids.sort_unstable();
        ensembl_ids.dedup();
        Self {
            efo_ids,
            ensembl_ids,
            enable_indirect,
        }
    }

    /// Whether the scope can't match anything, so the query can be skipped.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.efo_ids.is_empty() || self.ensembl_ids.is_empty() }

    /// `id_col IN (...)` keeping only `datasource`'s evidence for this scope.
    ///
    /// # Errors
    /// Never in practice; the subqueries and their binds are fixed here.
    pub fn candidate_filter<C: Column>(
        &self,
        id_col: C,
        datasource: &'static str,
    ) -> Result<Filter<C>> {
        let subquery = if self.enable_indirect {
            INDIRECT_EVIDENCE_IDS
        } else {
            DIRECT_EVIDENCE_IDS
        };
        Filter::in_subquery(
            id_col,
            subquery,
            vec![
                Value::Str(datasource.to_owned()),
                Value::StrArray(self.efo_ids.clone()),
                Value::StrArray(self.ensembl_ids.clone()),
            ],
        )
        .map_err(Error::new)
    }

    /// Filters keeping only `datasource`'s evidence for this scope, for a view whose source is
    /// `evidence_by_disease_and_target` itself, so ClickHouse reads it by its sorting key.
    ///
    /// # Errors
    /// If a column isn't `Str`.
    pub fn index_filters<C: Column>(
        &self,
        datasource_col: C,
        disease_col: C,
        target_col: C,
        datasource: &'static str,
    ) -> Result<Vec<Filter<C>>> {
        let disease = if self.enable_indirect {
            Filter::in_subquery(
                disease_col,
                DISEASES_AND_DESCENDANTS,
                vec![Value::StrArray(self.efo_ids.clone())],
            )
        } else {
            Filter::in_str(disease_col, self.efo_ids.clone())
        };
        Ok(vec![
            Filter::new(datasource_col, Op::Eq, Value::Str(datasource.to_owned()))
                .map_err(Error::new)?,
            disease.map_err(Error::new)?,
            Filter::in_str(target_col, self.ensembl_ids.clone()).map_err(Error::new)?,
        ])
    }
}

/// Sorted, searched and paged evidence tables, one field per datasource, all for the same
/// diseases and targets.
pub struct EvidenceViews {
    scope: EvidenceScope,
}

impl EvidenceViews {
    #[must_use]
    pub fn new(scope: EvidenceScope) -> Self { Self { scope } }
}

#[Object]
impl EvidenceViews {
    /// Clinical precedence evidence: drugs that target the target and are in clinical
    /// development or approved for the disease.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn clinical_precedence(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "clinical_precedence::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<clinical_precedence::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<clinical_precedence::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<clinical_precedence::EvidenceClinicalPrecedence>> {
        let key = clinical_precedence::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        clinical_precedence::load_evidence_clinical_precedence(ctx, key).await
    }

    /// GWAS credible sets evidence: fine-mapped GWAS signals whose locus-to-gene (L2G) model
    /// prioritises the target.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn gwas_credible_sets(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "gwas_credible_sets::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<gwas_credible_sets::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<gwas_credible_sets::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<gwas_credible_sets::EvidenceGwasCredibleSets>> {
        let key = gwas_credible_sets::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        gwas_credible_sets::load_evidence_gwas_credible_sets(ctx, key).await
    }

    /// Gene burden evidence: rare-variant collapsing (burden) tests.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn gene_burden(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "gene_burden::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<gene_burden::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<gene_burden::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<gene_burden::EvidenceGeneBurden>> {
        let key =
            gene_burden::QueryParams::new(self.scope.clone(), sort, search, page.index, page.size);
        gene_burden::load_evidence_gene_burden(ctx, key).await
    }

    /// Gene2Phenotype evidence: curated gene-disease relationships from the Gene2Phenotype
    /// panels.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn gene2phenotype(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "gene2phenotype::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<gene2phenotype::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<gene2phenotype::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<gene2phenotype::EvidenceGene2Phenotype>> {
        let key = gene2phenotype::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        gene2phenotype::load_evidence_gene2phenotype(ctx, key).await
    }

    /// `UniProt` literature evidence: disease/target relationships curated by `UniProt` from the
    /// literature.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn uniprot_literature(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "uniprot_literature::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<uniprot_literature::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<uniprot_literature::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<uniprot_literature::EvidenceUniprotLiterature>> {
        let key = uniprot_literature::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        uniprot_literature::load_evidence_uniprot_literature(ctx, key).await
    }

    /// `UniProt` curated variants evidence: disease-causing variants curated by `UniProt`.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn uniprot_variants(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "uniprot_variants::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<uniprot_variants::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<uniprot_variants::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<uniprot_variants::EvidenceUniprotVariants>> {
        let key = uniprot_variants::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        uniprot_variants::load_evidence_uniprot_variants(ctx, key).await
    }

    /// Genomics England `PanelApp` evidence: gene-disease relationships from expert-reviewed gene
    /// panels.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn genomics_england_panel_app(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "genomics_england_panel_app::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<genomics_england_panel_app::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<genomics_england_panel_app::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<genomics_england_panel_app::EvidenceGenomicsEnglandPanelApp>> {
        let key = genomics_england_panel_app::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        genomics_england_panel_app::load_evidence_genomics_england_panel_app(ctx, key).await
    }

    /// Expression Atlas evidence: genes differentially expressed between disease and normal
    /// samples in Expression Atlas experiments.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn expression_atlas(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "expression_atlas::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<expression_atlas::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<expression_atlas::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<expression_atlas::EvidenceExpressionAtlas>> {
        let key = expression_atlas::QueryParams::new(
            self.scope.clone(),
            sort,
            search,
            page.index,
            page.size,
        );
        expression_atlas::load_evidence_expression_atlas(ctx, key).await
    }

    /// Project Score (CRISPR screen) evidence: cancer dependency genes prioritised from CRISPR
    /// knockout screens of cancer cell lines.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn crispr(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "crispr::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<crispr::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<crispr::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<crispr::EvidenceCrispr>> {
        let key = crispr::QueryParams::new(self.scope.clone(), sort, search, page.index, page.size);
        crispr::load_evidence_crispr(ctx, key).await
    }

    /// Reactome evidence: reactions and pathways disrupted by variants in the target, linked to
    /// the disease.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn reactome(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "reactome::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<reactome::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<reactome::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<reactome::EvidenceReactome>> {
        let key =
            reactome::QueryParams::new(self.scope.clone(), sort, search, page.index, page.size);
        reactome::load_evidence_reactome(ctx, key).await
    }

    /// Orphanet evidence: rare-disease gene associations curated by Orphanet.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn orphanet(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "orphanet::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<orphanet::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<orphanet::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<orphanet::EvidenceOrphanet>> {
        let key =
            orphanet::QueryParams::new(self.scope.clone(), sort, search, page.index, page.size);
        orphanet::load_evidence_orphanet(ctx, key).await
    }

    /// ClinGen evidence: gene-disease validity classifications curated by ClinGen.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn clingen(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "clingen::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<clingen::SortField>>,
        #[graphql(desc = "Case-insensitive text search across the evidences' text fields. \
                          Searches all fields unless `fields` is given.")]
        search: Option<clingen::Search>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<clingen::EvidenceClinGen>> {
        let key =
            clingen::QueryParams::new(self.scope.clone(), sort, search, page.index, page.size);
        clingen::load_evidence_clingen(ctx, key).await
    }

    /// `ClinVar` (EVA) evidence: germline variants in the target with a clinical significance
    /// for the disease. Sorted by score only, with no search, so it's paged from the evidence
    /// index and only the page's rows are read in full.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn eva(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            default_with = "eva::SortField::default_sort()",
            validator(custom = "UniqueSortKeys"),
            desc = "List of sort statements. Each field can be used at most once."
        )]
        sort: Vec<Sort<eva::SortField>>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<eva::EvidenceEva>> {
        let key = eva::QueryParams::new(self.scope.clone(), sort, page.index, page.size);
        eva::load_evidence_eva(ctx, key).await
    }

    /// IMPC evidence: mouse models with a disrupted orthologue of the target whose phenotypes
    /// match the disease. Always sorted by score, highest first, with no search, so it's paged
    /// from the evidence index and only the page's rows are read in full.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn impc(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the evidence.")] page: Page,
    ) -> Result<Paged<impc::EvidenceImpc>> {
        let key = impc::QueryParams::new(self.scope.clone(), page.index, page.size);
        impc::load_evidence_impc(ctx, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::sql::{Column, ValueKind};

    #[derive(Clone, Copy)]
    struct Id;
    impl Column for Id {
        fn as_sql(&self) -> &'static str { "id" }
        fn kind(&self) -> ValueKind { ValueKind::Str }
    }

    #[test]
    fn scope_normalises_ids() {
        let a = EvidenceScope::new(
            vec!["b".into(), "a".into(), "b".into()],
            vec!["t".into()],
            true,
        );
        let b = EvidenceScope::new(
            vec!["a".into(), "b".into()],
            vec!["t".into(), "t".into()],
            true,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn both_subqueries_take_three_binds() {
        for indirect in [true, false] {
            let scope = EvidenceScope::new(vec!["d".into()], vec!["t".into()], indirect);
            assert!(scope.candidate_filter(Id, "clinical_precedence").is_ok());
        }
    }

    #[test]
    fn index_filters_are_datasource_disease_target() {
        for indirect in [true, false] {
            let scope = EvidenceScope::new(vec!["d".into()], vec!["t".into()], indirect);
            assert_eq!(scope.index_filters(Id, Id, Id, "eva").unwrap().len(), 3);
        }
    }

    #[test]
    fn empty_ids_make_an_empty_scope() {
        assert!(EvidenceScope::new(vec![], vec!["t".into()], true).is_empty());
        assert!(EvidenceScope::new(vec!["d".into()], vec![], true).is_empty());
        assert!(!EvidenceScope::new(vec!["d".into()], vec!["t".into()], true).is_empty());
    }
}
