# Open Targets nextgen Platform API

Rust GraphQL API for Open Targets Platform and Partner Preview Platform.

## Compatibility

This API version reads only ClickHouse databases loaded by `pos` from the [`opentargets/pipeline`](https://github.com/opentargets/pipeline) monorepo at **`pos@v26.12.0rc20` or later**. It does not work with databases loaded by `platform-output-support` or by earlier pos release candidates: rows would be misread silently. `scripts/clickhouse-from-pos.sh` builds an empty database for tests; `examples/check-clickhouse-enums.rs` checks the Rust enums against a database.

From rc20 (opentargets/pipeline#426) the pharmacogenomics table serves `pharmacogenomicsId` instead of `rowId`; rc17–rc19 databases fail the pharmacogenomics queries and live test.

## Testing against ClickHouse

Build an empty database from a pos image (starts ClickHouse in docker when no URL is given),
check the enums, then run the live queries in `tests/live/<product>/`:

```bash
scripts/clickhouse-from-pos.sh 26.12.0rc20 platform
cargo run --example check-clickhouse-enums --features product-platform -- http://localhost:8123
LIVE_CLICKHOUSE_URL=http://localhost:8123 cargo test --features product-platform --test live_clickhouse -- --ignored
```

`scripts/generate-live-queries.py` regenerates the query files from a product's schema.
`LIVE_FIXTURES=1` first replaces the rows of each table named by a `tests/live/fixtures/<table>.jsonl`
(scratch databases only: the tests refuse to TRUNCATE a database without the `_live_test_marker` table that
`scripts/clickhouse-from-pos.sh` creates). Without `LIVE_FIXTURES=1` the fixture tests skip with a message and
only `live_queries_return_no_errors` runs; add `LIVE_FIXTURES=1` to run them all.
`LIVE_IDS=tests/live/ids-2612.json` runs the queries against real ids (a real database, not with `LIVE_FIXTURES`).
Known pre-existing limit (BUG 1): an entity with enough evidence rows, such as ENSG00000149311 with
MONDO_0015356, makes the evidence `IN` list exceed ClickHouse `max_query_size`; that id set is kept out of the default run.

## Development

The repository pins Rust 1.99.0 in `rust-toolchain.toml`, matching the minimum Rust
version in `Cargo.toml`. Rustup selects the pinned toolchain for Cargo commands,
including the GraphQL pre-commit hook. Install the toolchain before committing:

```bash
rustup toolchain install 1.99.0 --profile minimal --component clippy
```

```bash
    cargo watch -c -w src -x 'run --features product-platform'
```

> [!NOTE]
> By default, Zed will work with the `product-platform` feature enabled. This means it will behave as if we're compiling
> the API for `platform` product. To work on `ppp`, change [Zed's settings](/.zed/settings.json) to:
> ```json
> {
>   "rust-analyzer": {
>      "initialization_options": {
>          "cargo": {
>             "features": ["ppp"]
> ```


## Timeout configuration

`clickhouse_max_execution_time` and `opensearch_timeout` accept nonnegative
Jiff duration strings, such as `"30s"`, `"500ms"`, `"1h 30m"`, or ISO 8601
`"PT30S"`. The same syntax applies to the `PLATFORM_API_` environment overrides.
When omitted, ClickHouse defaults to `0s` (no execution limit) and OpenSearch
defaults to `10s`.

Following the migration from humantime, units of days or larger are no longer
accepted for these fixed timeouts. Use `"24h"` instead of `"1d"`; months and years
are not supported.

## Build

Exactly one product feature must be enabled:

```bash
    cargo build --release --features product-platform
    cargo build --release --features product-ppp
```

Docker (`PRODUCT` defaults to `platform`):

```bash
    docker build -t platform-api .
    docker build --build-arg PRODUCT=ppp -t platform-api-ppp .
```

## Releases

Images are built by [`build-images.yaml`](.github/workflows/build-images.yaml), which is called by:

- `build-dev.yaml`, on pushing a `vX.Y.Z-*` tag. Pushes `<version>`.
- `build-release.yaml`, on publishing a GitHub release. Pushes `<version>`, plus `latest` unless it's a pre-release.

Every build includes both products, each to its own image:

| Product  | Image                      |
|----------|----------------------------|
| platform | `platform-api-nextgen`     |
| ppp      | `platform-api-nextgen-ppp` |

Both images share the tag's version. Each product is deployed independently.


## GraphQL schema tracking

Changes to Rust types can change the public GraphQL API: fields can disappear, argument types can change, and new fields can become available. Schema tracking makes API changes visible during review and provides schema release notes.

The idea is to keep a readable API snapshot alongside the Rust code. Before every commit, a local hook generates `schema.graphql` from the Platform Rust types and includes the snapshot in the commit. GitHub CI compares committed snapshots with GraphQL Inspector and reports breaking, potentially dangerous, and non-breaking
changes in PRs. Publishing a release generates schema release notes from the
previous and current release snapshots.

Schema reports are informational and require no approval.

Schema tracking covers `product-platform`.

### Set up

Install the Rust toolchain described under Development, then install the commit hook:

```bash
uv tool install pre-commit
pre-commit install
```


### Commit API changes

Stage the intended Rust changes and commit normally. The hook generates
`schema.graphql` and automatically stages the generated snapshot in the same commit.
The hook creates `schema.graphql` from the local Rust code when the snapshot is
missing and refreshes the snapshot on every commit.

Schema export runs without starting the API
server or connecting to databases. Export failures stop the commit before replacing
the snapshot. Schema changes never block the commit.

To regenerate and stage the schema from the current working tree manually:

```bash
pre-commit run graphql-schema --all-files
```

### Read PR reports

Schema CI compares committed snapshots with GraphQL Inspector:

| Event | Comparison |
| --- | --- |
| PR targeting `main` | Latest fetched `main` commit → PR head commit |
| Push to `main` | `main` before the push → pushed commit |
| Development-branch push without a PR to `main` | No schema comparison |

Pushing to a branch with an open PR targeting `main` triggers the PR comparison.
PRs targeting other branches, scheduled runs, and manual CI runs skip schema jobs.
CI reads `schema.graphql` directly from Git; CI does not compile Rust or modify
committed snapshots. Installing the commit hook keeps the snapshots current.

Reports appear in workflow logs, the job summary, and the
`graphql-schema-release-notes` artifact. For same-repository PRs, CI also updates
the PR description between `<!-- graphql-schema-notes:start -->` and
`<!-- graphql-schema-notes:end -->`. Description text outside the markers remains
unchanged. Fork and Dependabot PRs use the job summary and artifact because restricted
workflow tokens cannot update PR descriptions.

No differences produces a “No schema changes” report. Until the selected `main`
commit contains `schema.graphql`, PR and push comparisons download the baseline
from `https://api.platform.opentargets.xyz/latest/graphql/schema`. Reports identify
the deployment fallback and download time. Once `main` contains `schema.graphql`,
comparisons use the committed main snapshot. A missing snapshot in the PR or pushed
commit produces a “Schema comparison unavailable” report naming the missing side.
Release comparisons require saved snapshots from both release tags.
Schema jobs are non-blocking, including
reports containing breaking changes. Execution errors remain visible in workflow
logs. Other CI checks operate independently.

### Read release notes

Publishing a GitHub release triggers a comparison between the saved schemas in the
previous and new release tags. Stable releases compare with the previous published
stable release. Prereleases compare with the previous published release, including
prereleases. Release publication timestamps determine the previous release.

CI adds the schema report to the release description between the GraphQL schema
markers, preserving surrounding release text. The job summary and artifact also
contain the report. Schema reporting runs independently of release image builds.
The first release reports the absence of a previous release; missing historical
snapshots produce an unavailable-comparison report.
