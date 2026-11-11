//! Text search: one `{ text, fields }` input shared by every searchable table, whether it is
//! searched in memory or in `ClickHouse`.

use async_graphql::{Error, InputObject, InputType, Result};
use strum::IntoEnumIterator;

use crate::{
    entity::{
        disease::DiseaseSearchField, enhancer_to_gene::SearchField as EnhancerToGeneSearchField,
        pharmacogenomics::SearchField as PharmacogenomicsSearchField, study::StudySearchField,
    },
    query::sql::{Column, Filter},
    view::{
        disease_studies::SearchField as DiseaseStudiesSearchField,
        study_credible_sets::SearchField as StudyCredibleSetsSearchField,
        target_credible_sets::SearchField as TargetCredibleSetsSearchField,
        variant_credible_sets::SearchField as VariantCredibleSetsSearchField,
        variant_transcript_consequences::SearchField as VariantTranscriptConsequencesSearchField,
    },
};

/// An item that can be searched in memory, field by field.
pub trait Searchable {
    /// The fields `search` can match against.
    type Field: Copy;
    /// Whether `field` of this item contains `needle`, which is already trimmed and lowercase.
    fn field_matches(&self, field: Self::Field, needle: &str) -> bool;
}

/// A search field that names a `ClickHouse` column. `column_subset!` implements this.
pub trait SearchColumn: Copy {
    type Col: Column;
    /// The column this field refers to.
    fn as_col(self) -> Self::Col;
}

/// Whether `haystack` contains `needle`, ignoring case. `needle` must already be lowercase.
#[must_use]
pub fn contains(haystack: &str, needle: &str) -> bool { haystack.to_lowercase().contains(needle) }

/// Case-insensitive text search.
#[derive(Debug, Clone, Eq, PartialEq, Hash, InputObject)]
#[graphql(concrete(name = "DiseasesSearch", params(DiseaseSearchField)))]
#[graphql(concrete(name = "StudiesSearch", params(StudySearchField)))]
#[graphql(concrete(name = "PharmacogenomicsSearch", params(PharmacogenomicsSearchField)))]
#[graphql(concrete(name = "EnhancerToGeneSearch", params(EnhancerToGeneSearchField)))]
#[graphql(concrete(name = "DiseaseStudiesSearch", params(DiseaseStudiesSearchField)))]
#[graphql(concrete(name = "StudyCredibleSetsSearch", params(StudyCredibleSetsSearchField)))]
#[graphql(concrete(name = "TargetCredibleSetsSearch", params(TargetCredibleSetsSearchField)))]
#[graphql(concrete(name = "VariantCredibleSetsSearch", params(VariantCredibleSetsSearchField)))]
#[graphql(concrete(
    name = "VariantTranscriptConsequencesSearch",
    params(VariantTranscriptConsequencesSearchField)
))]
pub struct Search<F: InputType + IntoEnumIterator> {
    /// Text to look for. Matches if any of `fields` contains it.
    #[graphql(validator(chars_max_length = 200))]
    pub text: String,
    /// Fields to search in. Defaults to all of them.
    #[graphql(default_with = "F::iter().collect()", validator(min_items = 1))]
    pub fields: Vec<F>,
}

impl<F: InputType + IntoEnumIterator> Search<F> {
    /// Trims the text and sorts/dedupes the fields, so equivalent searches share
    /// a `DataLoader` key. A blank search becomes `None`, the same as no search.
    #[must_use]
    pub fn normalise(mut self) -> Option<Self>
    where
        F: Ord,
    {
        self.text = self.text.trim().to_owned();
        if self.text.is_empty() {
            return None;
        }
        self.fields.sort_unstable();
        self.fields.dedup();
        Some(self)
    }

    /// Whether any of the fields of `item` contains `needle`, which must be lowercase.
    pub fn matches<T>(&self, item: &T, needle: &str) -> bool
    where
        T: Searchable<Field = F>,
        F: Copy,
    {
        self.fields.iter().any(|&f| item.field_matches(f, needle))
    }

    /// The `ClickHouse` filter for this search.
    ///
    /// # Errors
    /// If a field's column can't be searched as text.
    pub fn to_filter(&self) -> Result<Filter<F::Col>>
    where
        F: SearchColumn,
    {
        let cols = self.fields.iter().map(|f| f.as_col()).collect();
        Filter::search(cols, self.text.clone()).map_err(Error::new)
    }
}
