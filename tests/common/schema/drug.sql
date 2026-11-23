-- ClickHouse schema for the `drug` table used by the integration tests.
--
-- The column list mirrors the `Drug` struct in `src/entity/drug.rs` field by
-- field, and the ClickHouse `clickhouse` crate matches tuple elements
-- *positionally*: a member that is added, removed or reordered here silently
-- decodes into the wrong field instead of failing. Keep this file and
-- `src/entity/drug.rs` in sync.
--
-- The names have to match the field names of the struct exactly, because the
-- API selects the columns by name, so the names have to match exactly.
--
-- `{namespace}` is replaced with the data namespace (the ClickHouse database
-- name) when the table is created.

CREATE TABLE {namespace}.drug (
    `id` String,
    `name` String,
    `synonyms` Array(Tuple(
        String, String
    )),
    `tradeNames` Array(Tuple(
        String, String
    )),
    `drugType` String,
    `crossReferences` Array(Tuple(
        String, Array(String)
    )),
    `parentId` Nullable(String),
    `childChemblIds` Array(Nullable(String)),
    `maximumClinicalStage` String,
    `description` Nullable(String),
    `molblock` Nullable(String),
    `pharmacogenomicsIds` Array(UInt64)
) ENGINE = Memory
