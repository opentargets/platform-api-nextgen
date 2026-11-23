-- ClickHouse schema for the `clinical_report` table used by the integration
-- tests.
--
-- The column list mirrors the `ClinicalReport` struct in
-- `src/entity/clinical_report.rs` field by field, and the ClickHouse
-- `clickhouse` crate matches tuple elements *positionally*: a member that is
-- added, removed or reordered here silently decodes into the wrong field
-- instead of failing. Keep this file and `src/entity/clinical_report.rs` in
-- sync.
--
-- The names have to match the field names of the struct exactly, because the
-- API selects the columns by name.
--
-- This is the production schema, taken from `platform-output-support`.
--
-- `{namespace}` is replaced with the data namespace (the ClickHouse database
-- name) when the table is created.

CREATE TABLE {namespace}.clinical_report (
    `id` String,
    `source` String,
    `clinicalStage` String,
    `origin` String,
    `phaseFromSource` Nullable(String),
    `provider` String,
    `type` Nullable(String),
    `title` Nullable(String),
    `trialStudyType` Nullable(String),
    `trialDescription` Nullable(String),
    `trialNumberOfArms` Nullable(Int32),
    `trialStartDate` Nullable(String),
    `trialLiterature` Array(Tuple(
        String, String
    )),
    `trialOverallStatus` Nullable(String),
    `trialWhyStopped` Nullable(String),
    `trialPrimaryPurpose` Nullable(String),
    `trialPhase` Nullable(String),
    `trialStopReasonCategories` Array(String),
    `trialSponsor` Tuple(
        Nullable(String), Nullable(String)
    ),
    `qualityControls` Array(String),
    `diseases` Array(Tuple(
        String, String
    )),
    `drugs` Array(Tuple(
        String, String
    )),
    `countries` Array(String),
    `year` Nullable(Int32),
    `sideEffects` Array(Tuple(
        Nullable(String), Nullable(String)
    )),
    `trialOfficialTitle` Nullable(String),
    `url` Nullable(String)
) ENGINE = Memory
