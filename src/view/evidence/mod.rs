//! One SQL-paged view per evidence datasource, each shaped like that datasource's table on the
//! evidence page.
//!
//! A datasource module doesn't know whether it is resolved from a disease or a target: its key
//! always carries an [`EvidenceScope`] with both id lists, and the entity fills in its own id.

use async_graphql::{Error, Result};

use crate::query::sql::{Column, Filter, Value};

pub mod clinical_precedence;

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
    fn empty_ids_make_an_empty_scope() {
        assert!(EvidenceScope::new(vec![], vec!["t".into()], true).is_empty());
        assert!(EvidenceScope::new(vec!["d".into()], vec![], true).is_empty());
        assert!(!EvidenceScope::new(vec!["d".into()], vec!["t".into()], true).is_empty());
    }
}
