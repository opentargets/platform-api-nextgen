-- ClickHouse schema for the `variants` ClickHouse table used by the integration
-- tests.
--
-- The column list mirrors the `Variant` struct in `src/entity/variant.rs` field
-- by field, and the ClickHouse `clickhouse` crate matches tuple elements
-- *positionally*: a member that is added, removed or reordered here silently
-- decodes into the wrong field instead of failing. Keep this file and
-- `src/entity/variant.rs` in sync.
--
-- The names have to match the field names of the struct exactly, because the
-- API selects the columns by name.
--
-- The chromosome is a number: the API decodes it as an enum.
--
-- `{namespace}` is replaced with the data namespace (the ClickHouse database
-- name) when the table is created.

CREATE TABLE {namespace}.variants (
    `variantId` String,
    `chromosome` Int8,
    `position` UInt32,
    `referenceAllele` String,
    `alternateAllele` String,
    `variantEffect` Array(Tuple(
        Nullable(String), Nullable(String), Nullable(Float64),
        Nullable(String), Nullable(String), Nullable(Float64)
    )),
    `transcriptConsequences` Array(Tuple(
        Array(String), Nullable(String), Array(String), Bool,
        Nullable(String), Int32, Int32, Nullable(String),
        Nullable(String), Nullable(String), Nullable(String),
        Nullable(Float64), Nullable(Float64), UInt32, Float64
    )),
    `rsIds` Array(String),
    `dbXrefs` Array(Tuple(
        Nullable(String), Nullable(String)
    )),
    `alleleFrequencies` Array(Tuple(
        Nullable(String), Nullable(Float64)
    )),
    `hgvsId` Nullable(String),
    `variantDescription` String,
    `mostSevereConsequenceId` String,
    `studyLocusIds` Array(String),
    `pharmacogenomicsIds` Array(UInt64)
) ENGINE = Memory
