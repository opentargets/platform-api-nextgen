use std::{char::CharTryFromError, cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{disease::{Disease, load_diseases}, sequence_ontology::{SequenceOntologyTerm, load_sequence_ontology_terms}, target::{Target, load_target}, variant::{Variant, load_variant}},
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        // search::Searchable,
        // sort::{Sort, SortKey},
    }
};



// ---- models ----


/// Data source information for protein coding coordinates.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Datasource {
    /// Count of evidence from this data source.
    datasource_count: u32,
    /// Identifier of the data source.
    datasource_id: String,
    /// Human-readable name of the data source.
    datasource_nice_name: String,
}


/// Protein coding coordinates linking this variant to its amino acid-level consequences in protein products. Describes variant consequences at the protein level including amino acid changes and their positions.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ProteinCodingCoordinates {
    #[graphql(skip)]
    target_id: String,
    ///UniProt protein accessions for the affected protein [bioregistry:uniprot].
    uniprot_accessions: Vec<String>,
    /// Position of the amino acid affected by the variant in the protein sequence.
    amino_acid_position: i32,
    /// Amino acid resulting from the variant.
    alternate_amino_acid: String,
    /// Reference amino acid at this position.
    reference_amino_acid: String,
    // #[graphql(skip)]
    variant_functional_consequence_ids: Vec<String>,
    /// Score indicating the predicted effect of the variant on the protein.
    variant_effect: Option<f64>,
    #[graphql(skip)]
    variant_id: String,
    #[graphql(skip)]
    diseases: Vec<String>,
    /// Data sources providing evidence for the protein coding coordinate.
    datasources: Vec<Datasource>,
    /// Therapeutic areas associated with the variant-consequence relationship.
    therapeutic_areas: Vec<String>,

}

// A ClickHouse row representing a protein coding coordinates.
#[derive(Debug, Row, Deserialize)]
struct ProteinCodingCoordinatesVariantRow {
    variant_id: String,
    protein_coding_coords: Vec<ProteinCodingCoordinates>,
}

// ---- loaders ----


pub struct ProteinCodingCoordinateVariantLoader {
    ch: ClickHouse,
}

impl ProteinCodingCoordinateVariantLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for ProteinCodingCoordinateVariantLoader {
    type Value = Vec<ProteinCodingCoordinates>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<ProteinCodingCoordinates>>, Self::Error> {
        let rows: Vec<ProteinCodingCoordinatesVariantRow> = self
            .ch
            .query("SELECT * FROM protein_coding_coords_by_variant WHERE variantId IN ?")
            .bind(ids)
            .fetch_all()
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.variant_id, r.protein_coding_coords))
            .collect())
    }
}


// --- resolvers ---

#[ComplexObject]
impl ProteinCodingCoordinates {
    /// Disease the protein coding variant has been associated with.
    async fn diseases(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.diseases).await
    }
    /// Target (gene/protein) the protein coding variant has been associated with.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        load_target(ctx, &self.target_id).await
    }
    /// Protein coding variant
    async fn variant(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Variant>> {
        load_variant(ctx, &self.variant_id).await
    }
    /// The sequence ontology term capturing the consequence of the variant based on Ensembl VEP in the context of the transcript [bioregistry:so].\
    async fn variant_consequences(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<SequenceOntologyTerm>> {
        load_sequence_ontology_terms(ctx, &self.variant_functional_consequence_ids.iter().map(|id| id.replace('_', ":")).collect::<Vec<String>>()).await
    }
}
