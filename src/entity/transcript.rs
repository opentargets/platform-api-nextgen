//! Ensembl transcripts of a target, read from the `transcript` table.

use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{target::Strand, variant::Chromosome},
    query::paginate::{Page, Paged},
};

// ---- models ----

/// Ensembl transcript flag, such as `mane` or `appris`.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct Flag {
    /// Flag name.
    label: Option<String>,
    /// Flag value.
    value: Option<String>,
}

/// Exon of a transcript.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Exon {
    /// Ensembl exon identifier.
    #[allow(clippy::struct_field_names)]
    exon_id: String,
    #[graphql(skip)]
    chromosome: Chromosome,
    /// Genomic start position of the exon.
    start: u32,
    /// Genomic end position of the exon.
    end: u32,
    #[graphql(skip)]
    strand: i8,
}

#[ComplexObject]
impl Exon {
    /// Chromosome the exon is located on.
    async fn chromosome(&self) -> String { chromosome_label(self.chromosome) }

    /// Strand orientation of the exon.
    async fn strand(&self) -> Strand { Strand::from(self.strand) }
}

/// Database cross-reference of a transcript. Read to match the table layout, not exposed.
#[derive(Debug, Clone, Deserialize)]
struct DbXref {
    #[allow(dead_code, reason = "read only to keep the tuple layout")]
    id: String,
    #[allow(dead_code, reason = "read only to keep the tuple layout")]
    source: String,
}

// Field order must match the `transcripts` tuple in the `transcript` table: nested tuples are read
// positionally.
/// Ensembl transcript of a target gene, with protein, structure and exon annotations.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Transcript {
    /// Ensembl transcript identifier.
    #[allow(clippy::struct_field_names)]
    transcript_id: String,
    /// Biotype classification of the transcript.
    biotype: String,
    /// Ensembl protein (translation) identifier.
    protein_id: Option<String>,
    /// Reviewed (Swiss-Prot) UniProt accessions mapped to the transcript.
    uniprot_swissprot_ids: Vec<String>,
    /// Unreviewed (`TrEMBL`) UniProt accessions mapped to the transcript.
    uniprot_trembl_ids: Vec<String>,
    /// UniProt isoform identifiers mapped to the transcript.
    uniprot_isoform_ids: Vec<String>,
    /// AlphaFold structure prediction identifiers.
    alphafold_ids: Vec<String>,
    #[graphql(skip)]
    #[allow(dead_code, reason = "read only to keep the tuple layout")]
    db_xrefs: Vec<DbXref>,
    /// Whether this is the Ensembl canonical transcript.
    is_ensembl_canonical: bool,
    #[graphql(skip)]
    chromosome: Chromosome,
    /// Genomic start position of the transcript.
    start: u32,
    /// Genomic end position of the transcript.
    end: u32,
    #[graphql(skip)]
    strand: i8,
    /// Genomic position of the transcription start site.
    transcription_start_site: u32,
    /// Ensembl transcript flags, such as MANE select or APPRIS principal.
    flags: Vec<Flag>,
    /// Exons of the transcript.
    exons: Vec<Exon>,
}

#[ComplexObject]
impl Transcript {
    /// Chromosome the transcript is located on.
    async fn chromosome(&self) -> String { chromosome_label(self.chromosome) }

    /// Strand orientation of the transcript.
    async fn strand(&self) -> Strand { Strand::from(self.strand) }
}

/// Chromosome name as Ensembl writes it: `1`..`22`, `X`, `Y`, `MT`.
fn chromosome_label(chromosome: Chromosome) -> String {
    match chromosome {
        Chromosome::ChrX => "X".to_owned(),
        Chromosome::ChrY => "Y".to_owned(),
        Chromosome::ChrMT => "MT".to_owned(),
        other => (other as i8).to_string(),
    }
}

// ---- loaders ----

/// One page of a target's transcripts, optionally restricted to (non-)canonical ones.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TranscriptsKey {
    target_id: String,
    canonical: Option<bool>,
    page: Page,
}

#[derive(Debug, Clone, Row, Deserialize)]
struct TranscriptsRow {
    #[serde(rename = "targetId")]
    target_id: String,
    count: u64,
    rows: Vec<Transcript>,
}

#[derive(From)]
pub struct TranscriptsLoader {
    ch: ClickHouse,
}

impl TranscriptsLoader {
    /// Transcripts of `target_ids`, filtered on `canonical` and paged in `ClickHouse`.
    async fn fetch(
        &self,
        target_ids: &[&str],
        canonical: Option<bool>,
        page: Page,
    ) -> async_graphql::Result<Vec<TranscriptsRow>> {
        let filtered = match canonical {
            None => "transcripts",
            Some(true) => "arrayFilter(t -> t.isEnsemblCanonical, transcripts)",
            Some(false) => "arrayFilter(t -> NOT t.isEnsemblCanonical, transcripts)",
        };
        let sql = format!(
            "WITH {filtered} AS filtered
             SELECT targetId, length(filtered) AS count, arraySlice(filtered, ?, ?) AS rows
             FROM transcript
             WHERE targetId IN ?"
        );
        // `arraySlice` is 1-based.
        let offset = u64::from(page.index) * u64::from(page.size) + 1;
        self.ch
            .query(&sql)
            .bind(offset)
            .bind(page.size)
            .bind(target_ids)
            .fetch_all::<TranscriptsRow>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<TranscriptsKey> for TranscriptsLoader {
    type Value = Paged<Transcript>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[TranscriptsKey],
    ) -> Result<HashMap<TranscriptsKey, Self::Value>, Self::Error> {
        // Keys sharing a filter and page go in one query; usually there is only one group.
        let mut groups: HashMap<(Option<bool>, Page), Vec<&str>> = HashMap::new();
        for key in keys {
            groups
                .entry((key.canonical, key.page))
                .or_default()
                .push(&key.target_id);
        }

        let mut out = HashMap::with_capacity(keys.len());
        for ((canonical, page), target_ids) in groups {
            for TranscriptsRow {
                target_id,
                count,
                rows,
            } in self.fetch(&target_ids, canonical, page).await?
            {
                let key = TranscriptsKey {
                    target_id,
                    canonical,
                    page,
                };
                out.insert(key, Paged { count, rows });
            }
        }
        Ok(out)
    }
}

/// Load one page of a target's transcripts. Targets without transcripts get an empty page.
///
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_transcripts(
    ctx: &Context<'_>,
    target_id: String,
    canonical: Option<bool>,
    page: Page,
) -> async_graphql::Result<Paged<Transcript>> {
    Ok(ctx
        .data_unchecked::<DataLoader<TranscriptsLoader>>()
        .load_one(TranscriptsKey {
            target_id,
            canonical,
            page,
        })
        .await?
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromosome_label_matches_ensembl_names() {
        assert_eq!(chromosome_label(Chromosome::Chr1), "1");
        assert_eq!(chromosome_label(Chromosome::Chr22), "22");
        assert_eq!(chromosome_label(Chromosome::ChrX), "X");
        assert_eq!(chromosome_label(Chromosome::ChrY), "Y");
        assert_eq!(chromosome_label(Chromosome::ChrMT), "MT");
    }
}
