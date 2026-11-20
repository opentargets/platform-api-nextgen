-- ClickHouse schema for the `targets` table used by the integration tests.
--
-- The column list mirrors the `Target` struct in `src/entity/target.rs` field by
-- field, and the ClickHouse `clickhouse` crate matches tuple elements
-- *positionally*: a member that is added, removed or reordered here silently
-- decodes into the wrong field instead of failing. Keep this file and
-- `src/entity/target.rs` in sync.
--
-- `{namespace}` is replaced with the data namespace (the ClickHouse database
-- name) when the table is created.
--
-- The `transcripts` boolean columns are nullable because the API models them as
-- `Option<bool>`, and the sample data has rows where they are absent.

CREATE TABLE {namespace}.targets (
    `id` String,
    `alternativeGenes` Array(String),
    `approvedSymbol` String,
    `approvedName` String,
    `biotype` String,
    `canonicalTranscript` Tuple(
        String, String, Int32, Int32, Int8
    ),
    `chemicalProbes` Array(Tuple(
        String, Nullable(String), Nullable(String), Nullable(String),
        Array(String), Bool, Array(String), Nullable(Float64),
        Nullable(Float64), Nullable(Float64), String,
        Array(Tuple(String, Nullable(String)))
    )),
    `dbXrefs` Array(Tuple(String, String)),
    `functionDescriptions` Array(String),
    `constraint` Array(Tuple(
        String, Nullable(Float64), Nullable(UInt32), Nullable(Float64),
        Nullable(Float64), Nullable(Float64), Nullable(Float64),
        Nullable(UInt32), Nullable(UInt32), Nullable(UInt32)
    )),
    `genomicLocation` Tuple(String, UInt32, UInt32, Int8),
    `go` Array(Tuple(String, String, String, String, String)),
    `hallmarks` Tuple(
        Array(Tuple(String, Nullable(String), String, UInt32)),
        Array(Tuple(String, String, Nullable(UInt32)))
    ),
    `homologues` Array(Tuple(
        String, Float64, String, String, String, String, Float64, Nullable(String)
    )),
    `pathways` Array(Tuple(String, String, String)),
    `proteinIds` Array(Tuple(String, String)),
    `safetyLiabilities` Array(Tuple(
        Array(Tuple(
            Nullable(String), Nullable(String), Nullable(String),
            Nullable(String), Nullable(String)
        )),
        String,
        Array(Tuple(String, String)),
        Nullable(String), Nullable(String), Nullable(String), Nullable(String),
        Array(Tuple(Nullable(String), Nullable(String), Nullable(String)))
    )),
    `studyLocusIds` Array(String),
    `pharmacogenomicsIds` Array(UInt64),
    `subcellularLocations` Array(Tuple(
        String, String, Nullable(String), Nullable(String), Nullable(String)
    )),
    `synonyms` Array(Tuple(String, String)),
    `symbolSynonyms` Array(Tuple(String, String)),
    `nameSynonyms` Array(Tuple(String, String)),
    `obsoleteSymbols` Array(Tuple(String, String)),
    `obsoleteNames` Array(Tuple(String, String)),
    `targetClass` Array(Tuple(UInt32, String, String)),
    `tractability` Array(Tuple(String, String, Bool)),
    `transcriptIds` Array(String),
    `transcripts` Array(Tuple(
        String, String, Nullable(Bool), Nullable(String), Nullable(Bool),
        Nullable(String), Nullable(String), Nullable(String)
    ))
) ENGINE = Memory
