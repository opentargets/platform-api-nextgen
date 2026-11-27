use std::{cmp::Ordering, collections::HashSet, hash::Hash};

use async_graphql::{CustomValidator, Enum, InputObject, InputType, InputValueError};

use crate::{
    entity::{
        clinical_indication::{
            SortField as ClinicalIndicationSortField, by_drug::SortField as DrugIndicationSortField,
        },
        clinical_target::SortField as ClinicalTargetSortField,
        disease::DiseaseSortField,
        enhancer_to_gene::SortField as EnhancerToGeneSortField,
        hpo::HpoSortField,
        pharmacogenomics::SortField as PharmacogenomicsSortField,
        study::StudySortField,
        variant::TranscriptConsequenceSortField,
    },
    query::Entity,
    view::{
        credible_set_colocalisations::SortField as CredibleSetColocalisationsSortField,
        credible_set_variants::SortField as CredibleSetVariantsSortField,
        disease_studies::SortField as DiseaseStudiesSortField,
        evidence::{
            cancer_gene_census::SortField as EvidenceCancerGeneCensusSortField,
            clingen::SortField as EvidenceClinGenSortField,
            clinical_precedence::SortField as EvidenceClinicalPrecedenceSortField,
            crispr::SortField as EvidenceCrisprSortField,
            crispr_screen::SortField as EvidenceCrisprScreenSortField,
            eva::SortField as EvidenceEvaSortField,
            expression_atlas::SortField as EvidenceExpressionAtlasSortField,
            gene_burden::SortField as EvidenceGeneBurdenSortField,
            gene2phenotype::SortField as EvidenceGene2PhenotypeSortField,
            genomics_england_panel_app::SortField as EvidenceGenomicsEnglandPanelAppSortField,
            gwas_credible_sets::SortField as EvidenceGwasCredibleSetsSortField,
            intogen::SortField as EvidenceIntogenSortField,
            orphanet::SortField as EvidenceOrphanetSortField,
            reactome::SortField as EvidenceReactomeSortField,
            uniprot_literature::SortField as EvidenceUniprotLiteratureSortField,
            uniprot_variants::SortField as EvidenceUniprotVariantsSortField,
        },
        study_credible_sets::SortField as StudyCredibleSetsSortField,
        target_credible_sets::SortField as TargetCredibleSetsSortField,
        variant_credible_sets::SortField as VariantCredibleSetsSortField,
        variant_transcript_consequences::SortField as VariantTranscriptConsequencesSortField,
    },
};

/// Sort direction: ascending or descending.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Default, Hash)]
pub enum SortDirection {
    /// Ascending order.
    #[default]
    Ascending,
    /// Descending order.
    Descending,
}
impl SortDirection {
    #[must_use]
    pub fn as_sql(self) -> &'static str {
        match self {
            SortDirection::Ascending => "ASC",
            SortDirection::Descending => "DESC",
        }
    }
}

/// Sort types. Contain the sort field and direction.
#[derive(Debug, Clone, Copy, InputObject, Default, Eq, PartialEq, Hash)]
#[graphql(concrete(name = "DiseaseSort", params(DiseaseSortField)))]
#[graphql(concrete(name = "ClinicalIndicationSort", params(ClinicalIndicationSortField)))]
#[graphql(concrete(name = "ClinicalTargetSort", params(ClinicalTargetSortField)))]
#[graphql(concrete(name = "DrugIndicationSort", params(DrugIndicationSortField)))]
#[graphql(concrete(name = "HpoSort", params(HpoSortField)))]
#[graphql(concrete(name = "StudySort", params(StudySortField)))]
#[graphql(concrete(name = "TranscriptConsequenceSort", params(TranscriptConsequenceSortField)))]
#[graphql(concrete(name = "VariantCredibleSetsSort", params(VariantCredibleSetsSortField)))]
#[graphql(concrete(name = "DiseaseStudiesSort", params(DiseaseStudiesSortField)))]
#[graphql(concrete(name = "StudyCredibleSetsSort", params(StudyCredibleSetsSortField)))]
#[graphql(concrete(name = "CredibleSetVariantsSort", params(CredibleSetVariantsSortField)))]
#[graphql(concrete(
    name = "CredibleSetColocalisationsSort",
    params(CredibleSetColocalisationsSortField)
))]
#[graphql(concrete(name = "TargetCredibleSetsSort", params(TargetCredibleSetsSortField)))]
#[graphql(concrete(name = "PharmacogenomicsSort", params(PharmacogenomicsSortField)))]
#[graphql(concrete(name = "EnhancerToGeneSort", params(EnhancerToGeneSortField)))]
#[graphql(concrete(
    name = "EvidenceClinicalPrecedenceSort",
    params(EvidenceClinicalPrecedenceSortField)
))]
#[graphql(concrete(
    name = "EvidenceGwasCredibleSetsSort",
    params(EvidenceGwasCredibleSetsSortField)
))]
#[graphql(concrete(name = "EvidenceEvaSort", params(EvidenceEvaSortField)))]
#[graphql(concrete(name = "EvidenceGene2PhenotypeSort", params(EvidenceGene2PhenotypeSortField)))]
#[graphql(concrete(
    name = "EvidenceUniprotLiteratureSort",
    params(EvidenceUniprotLiteratureSortField)
))]
#[graphql(concrete(
    name = "EvidenceUniprotVariantsSort",
    params(EvidenceUniprotVariantsSortField)
))]
#[graphql(concrete(name = "EvidenceGeneBurdenSort", params(EvidenceGeneBurdenSortField)))]
#[graphql(concrete(name = "EvidenceOrphanetSort", params(EvidenceOrphanetSortField)))]
#[graphql(concrete(name = "EvidenceReactomeSort", params(EvidenceReactomeSortField)))]
#[graphql(concrete(name = "EvidenceCrisprSort", params(EvidenceCrisprSortField)))]
#[graphql(concrete(name = "EvidenceCrisprScreenSort", params(EvidenceCrisprScreenSortField)))]
#[graphql(concrete(name = "EvidenceIntogenSort", params(EvidenceIntogenSortField)))]
#[graphql(concrete(name = "EvidenceClinGenSort", params(EvidenceClinGenSortField)))]
#[graphql(concrete(
    name = "EvidenceCancerGeneCensusSort",
    params(EvidenceCancerGeneCensusSortField)
))]
#[graphql(concrete(
    name = "EvidenceExpressionAtlasSort",
    params(EvidenceExpressionAtlasSortField)
))]
#[graphql(concrete(
    name = "EvidenceGenomicsEnglandPanelAppSort",
    params(EvidenceGenomicsEnglandPanelAppSortField)
))]
#[graphql(concrete(
    name = "VariantTranscriptConsequencesSort",
    params(VariantTranscriptConsequencesSortField)
))]
pub struct Sort<K: InputType> {
    /// The field to sort by.
    pub key: K,
    /// The direction to sort in.
    #[graphql(default)]
    pub direction: SortDirection,
}

/// GraphQL argument validator: rejects a sort list that uses the same field
/// more than once, even with different directions.
///
/// ```ignore
/// #[graphql(validator(custom = "UniqueSortKeys"))] sort: Vec<Sort<MySortField>>,
/// ```
pub struct UniqueSortKeys;

impl<K> CustomValidator<Vec<Sort<K>>> for UniqueSortKeys
where
    K: InputType + Copy + Eq + Hash,
    // `Sort<K>` is only a GraphQL type for the `concrete(...)` keys listed above.
    Vec<Sort<K>>: InputType,
{
    fn check(&self, sorts: &Vec<Sort<K>>) -> Result<(), InputValueError<Vec<Sort<K>>>> {
        let mut seen = HashSet::new();
        match sorts.iter().find(|s| !seen.insert(s.key)) {
            Some(dup) => Err(InputValueError::custom(format!(
                "sort field {} can only be used once",
                dup.key.to_value()
            ))),
            None => Ok(()),
        }
    }
}

/// Contains the compare function for sorting values on each of the sort fields.
pub trait SortKey<T> {
    /// Compares two values of type `T` using the given sort direction.
    fn compare(&self, a: &T, b: &T, direction: SortDirection) -> Ordering;
}

/// Null-object sort key.
///
/// Used when we don't want to define sort keys.
impl<T> SortKey<T> for NoSort {
    fn compare(&self, _: &T, _: &T, _: SortDirection) -> Ordering { Ordering::Equal }
}

/// Null-object sort key.
///
/// Used when we don't want to define sort keys.
#[derive(Clone, Copy)]
pub struct NoSort;

/// Where nulls go, independent of direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nulls {
    /// Nulls come first, before non-nulls.
    First,
    /// Nulls come last, after non-nulls.
    #[default]
    Last,
}

/// One term of an ordering, like `x DESC NULLS LAST` in SQL.
#[derive(Debug, Clone, Copy)]
pub struct Term {
    /// The sort direction.
    pub direction: SortDirection,
    /// Where nulls go, independent of direction.
    pub nulls: Nulls,
}

impl Term {
    #[must_use]
    pub const fn new(direction: SortDirection, nulls: Nulls) -> Self { Self { direction, nulls } }

    /// Compares two values using the sort direction.
    pub fn cmp<V: Ord + ?Sized>(self, a: &V, b: &V) -> Ordering {
        match self.direction {
            SortDirection::Ascending => a.cmp(b),
            SortDirection::Descending => b.cmp(a),
        }
    }

    /// Compares two optional values using the sort direction and nulls ordering.
    pub fn cmp_opt<V: Ord>(self, a: &Option<V>, b: &Option<V>) -> Ordering {
        let null_vs_value = match self.nulls {
            Nulls::First => Ordering::Less,
            Nulls::Last => Ordering::Greater,
        };
        match (a, b) {
            (Some(x), Some(y)) => self.cmp(x, y),
            (None, Some(_)) => null_vs_value,
            (Some(_), None) => null_vs_value.reverse(),
            (None, None) => Ordering::Equal,
        }
    }
}

/// Sorts a slice of items using the given sort terms.
pub fn sort_items<T, K>(items: &mut [T], sorts: &[Sort<K>])
where
    T: Entity,
    K: InputType + SortKey<T>,
{
    items.sort_unstable_by(|a, b| {
        sorts
            .iter()
            .fold(Ordering::Equal, |acc, s| {
                acc.then_with(|| s.key.compare(a, b, s.direction))
            })
            .then_with(|| a.id().cmp(b.id()))
    });
}

#[cfg(test)]
mod tests {
    use SortDirection::{Ascending, Descending};

    use super::*;

    #[derive(Clone)]
    struct Item {
        id: &'static str,
        group: i32,
        score: i32,
    }

    impl Entity for Item {
        fn id(&self) -> &str { self.id }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Enum)]
    enum Field {
        Group,
        Score,
    }

    impl SortKey<Item> for Field {
        fn compare(&self, a: &Item, b: &Item, direction: SortDirection) -> Ordering {
            let term = Term::new(direction, Nulls::Last);
            match self {
                Self::Group => term.cmp(&a.group, &b.group),
                Self::Score => term.cmp(&a.score, &b.score),
            }
        }
    }

    #[test]
    fn sort_key_order_and_each_keys_direction_determine_row_order() {
        use Field::{Group, Score};

        let items: Vec<_> = [
            ("e", 1, 20),
            ("b", 1, 20),
            ("a", 2, 10),
            ("d", 1, 10),
            ("c", 2, 30),
        ]
        .into_iter()
        .map(|(id, group, score)| Item { id, group, score })
        .collect();
        for (terms, expected) in [
            (
                [(Group, Ascending), (Score, Descending)],
                ["b", "e", "d", "c", "a"],
            ),
            (
                [(Score, Descending), (Group, Ascending)],
                ["c", "b", "e", "d", "a"],
            ),
            (
                [(Group, Descending), (Score, Ascending)],
                ["a", "c", "d", "b", "e"],
            ),
        ] {
            let sorts = terms.map(|(key, direction)| Sort { key, direction });
            let mut sorted = items.clone();
            sort_items(&mut sorted, &sorts);
            assert_eq!(sorted.iter().map(Entity::id).collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn tied_keys_and_empty_sort_lists_use_ascending_ids_independent_of_input_order() {
        for sorts in [
            vec![],
            vec![Sort {
                key: Field::Score,
                direction: Ascending,
            }],
            vec![Sort {
                key: Field::Score,
                direction: Descending,
            }],
        ] {
            for ids in [["c", "a", "b"], ["b", "a", "c"]] {
                let mut items = ids.map(|id| Item {
                    id,
                    group: 1,
                    score: 10,
                });
                sort_items(&mut items, &sorts);
                assert_eq!(items.map(|item| item.id), ["a", "b", "c"]);
            }
        }
    }

    #[test]
    fn null_placement_is_independent_of_sort_direction() {
        for (direction, nulls, expected) in [
            (
                Ascending,
                Nulls::First,
                [None, None, Some(-1), Some(2), Some(2)],
            ),
            (
                Descending,
                Nulls::First,
                [None, None, Some(2), Some(2), Some(-1)],
            ),
            (
                Ascending,
                Nulls::Last,
                [Some(-1), Some(2), Some(2), None, None],
            ),
            (
                Descending,
                Nulls::Last,
                [Some(2), Some(2), Some(-1), None, None],
            ),
        ] {
            let term = Term::new(direction, nulls);
            let mut values = [Some(2), None, Some(-1), None, Some(2)];
            values.sort_unstable_by(|a, b| term.cmp_opt(a, b));
            assert_eq!(values, expected, "{term:?}");
            assert_eq!(
                term.cmp_opt::<i32>(&None, &None),
                Ordering::Equal,
                "{term:?}"
            );
            assert_eq!(
                term.cmp_opt(&Some(2), &Some(2)),
                Ordering::Equal,
                "{term:?}"
            );
        }
    }

    #[test]
    fn unique_sort_keys_accepts_empty_single_and_distinct_field_lists() {
        use DiseaseStudiesSortField::{NSamples, PublicationDate};

        assert!(
            UniqueSortKeys
                .check(&Vec::<Sort<DiseaseStudiesSortField>>::new())
                .is_ok()
        );
        for direction in [Ascending, Descending] {
            let first = Sort {
                key: NSamples,
                direction,
            };
            assert!(UniqueSortKeys.check(&vec![first]).is_ok());
            assert!(
                UniqueSortKeys
                    .check(&vec![
                        first,
                        Sort {
                            key: PublicationDate,
                            direction
                        }
                    ])
                    .is_ok()
            );
        }
    }

    #[test]
    fn unique_sort_keys_rejects_repeated_fields_even_with_different_directions() {
        use DiseaseStudiesSortField::{NSamples, PublicationDate};

        for direction in [Ascending, Descending] {
            let sorts = vec![
                Sort {
                    key: NSamples,
                    direction: Ascending,
                },
                Sort {
                    key: PublicationDate,
                    direction: Ascending,
                },
                Sort {
                    key: NSamples,
                    direction,
                },
            ];
            assert!(UniqueSortKeys.check(&sorts).is_err(), "{sorts:?}");
        }
    }
}
