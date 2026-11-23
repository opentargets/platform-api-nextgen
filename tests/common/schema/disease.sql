-- ClickHouse schema for the `disease` table used by the integration tests.
--
-- The column list mirrors the `Disease` struct in `src/entity/disease.rs` field
-- by field, and the ClickHouse `clickhouse` crate matches tuple elements
-- *positionally*: a member that is added, removed or reordered here silently
-- decodes into the wrong field instead of failing. Keep this file and
-- `src/entity/disease.rs` in sync.
--
-- The names have to match the field names of the struct exactly, because the
-- API selects the columns by name, so the names have to match exactly.
--
-- This is not a copy of the production `disease` table: production keeps the
-- synonyms in four separate arrays and the therapeutic area flag inside a
-- tuple, and transforms both into the shape the API expects with SQL. The
-- table here holds the shape the API expects already.
--
-- The study and location identifiers are empty: in production they come from
-- joins with the studies of a disease.
--
-- `{namespace}` is replaced with the data namespace (the ClickHouse database
-- name) when the table is created.

CREATE TABLE {namespace}.disease (
    `id` String,
    `name` String,
    `description` Nullable(String),
    `isTherapeuticArea` Bool,
    `therapeuticAreas` Array(String),
    `parents` Array(String),
    `children` Array(String),
    `ancestors` Array(String),
    `descendants` Array(String),
    `synonyms` Array(Tuple(
        String, Array(Nullable(String))
    )),
    `obsoleteTerms` Array(String),
    `dbXRefs` Array(String),
    `directLocationIds` Array(String),
    `indirectLocationIds` Array(String),
    `studyIds` Array(String),
    `indirectStudyIds` Array(String)
) ENGINE = Memory
