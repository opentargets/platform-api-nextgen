-- ClickHouse schema for the `study` table used by the integration tests.
--
-- The column list mirrors the `Study` struct in `src/entity/study.rs` field by
-- field, and the ClickHouse `clickhouse` crate matches tuple elements
-- *positionally*: a member that is added, removed or reordered here silently
-- decodes into the wrong field instead of failing. Keep this file and
-- `src/entity/study.rs` in sync.
--
-- The names have to match the field names of the struct exactly, because the
-- API selects the columns by name.
--
-- `{namespace}` is replaced with the data namespace (the ClickHouse database
-- name) when the table is created.

CREATE TABLE {namespace}.studies (
    `studyId` String,
    `projectId` String,
    `studyType` Int8,
    `traitFromSource` String,
    `traitFromSourceMappedIds` Array(String),
    `condition` Nullable(String),
    `initialSampleSize` Nullable(String),
    `nSamples` Nullable(UInt32),
    `nCases` Nullable(UInt32),
    `nControls` Nullable(UInt32),
    `cohorts` Array(String),
    `discoverySamples` Array(Tuple(
        String, UInt32
    )),
    `replicationSamples` Array(Tuple(
        String, UInt32
    )),
    `ldPopulationStructure` Array(Tuple(
        String, Nullable(Float64)
    )),
    `hasSumstats` Nullable(Bool),
    `summarystatsLocation` Nullable(String),
    `analysisFlags` Array(String),
    `qualityControls` Array(String),
    `sumstatQCValues` Array(Tuple(
        String, Float64
    )),    `pubmedId` Nullable(String),
    `publicationFirstAuthor` Nullable(String),
    `publicationTitle` Nullable(String),
    `publicationJournal` Nullable(String),
    `publicationDate` Nullable(String),
    `diseaseIds` Array(String),
    `studyLocusIds` Array(String),
    `geneId` Nullable(String),
    `biosampleFromSourceId` Nullable(String),
    `backgroundTraitFromSourceMappedIds` Array(String)
) ENGINE = Memory
