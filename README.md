# Open Targets nextgen Platform API

Rust GraphQL API for Open Targets Platform and Partner Preview Platform.

## Development

The repository pins Rust 1.99.0 in `rust-toolchain.toml`, matching the minimum Rust
version in `Cargo.toml`. Rustup selects the pinned toolchain for Cargo commands,
including the GraphQL pre-commit hook. Install the toolchain before committing:

```bash
rustup toolchain install 1.99.0 --profile minimal --component clippy
```

Install [`just`](https://just.systems) once (`uv tool install rust-just`, or `brew install just`),
then run `just setup` to install `pre-commit`, `cargo-release` and the commit hook. Run `just`
on its own to list every recipe.

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
    docker build --build-arg APP_VERSION=26.12.0-dev.local.1 -t platform-api .   # optional; defaults to Cargo.toml
```

## Versioning and releases

The version in `Cargo.toml` is the single source of truth. Between releases it holds the
**next** version with a `-dev` suffix (e.g. `26.12.0-dev`). Only `cargo release`, run
through the `just` recipes below, changes it and creates `v*` tags. The API reports the exact
version it was built with (`meta { name apiVersion }`).

| Kind | Version | Image tags | How |
|---|---|---|---|
| Dev | `26.12.0-dev.<id>.<run>` | `26.12.0-dev.4628.57`, `dev-4628` | `just dev-build` on your branch |
| Release candidate | `26.12.0-rc.1` | `26.12.0-rc.1` | `just rc` on `main` |
| Release | `26.12.0` | `26.12.0`, `latest` | `just release` on `main` |

Semver orders these `26.12.0-dev < 26.12.0-dev.* < 26.12.0-rc.N < 26.12.0`. Dev builds
have no meaningful order between themselves.

Every build includes both products, each to its own image, pushed to GHCR and GAR:

| Product  | Image                      |
|----------|----------------------------|
| platform | `platform-api-nextgen`     |
| ppp      | `platform-api-nextgen-ppp` |

Both images share the version. Each product is deployed independently. The images are built
by [`build-images.yaml`](.github/workflows/build-images.yaml), which is called by
[`build-dev.yaml`](.github/workflows/build-dev.yaml) (on demand) and
[`build-release.yaml`](.github/workflows/build-release.yaml) (on a `v*` tag push).

### Rules

- Never edit the version in `Cargo.toml` by hand, never push a `v*` tag by hand, and never
  create a GitHub release by hand. CI rejects tags that don't match `Cargo.toml` or aren't on `main`.
- Start branch names with the issue number (`4628-custom-views`, `feature/4628-x`), or use a
  short name (`fix-typo`). Don't name release branches `<number>-release`, because the number is
  read as an issue id. Use `release/26.12` instead.
- Use dev images for features, RC images for staging, and release images in production.

### Dev builds

Pushing never builds an image. Push your branch, then:

| Command | Runs |
|---|---|
| `just dev-build` | `gh workflow run build-dev.yaml --ref <current branch>` |
| `just dev-build 4628` | `gh workflow run build-dev.yaml --ref <current branch> -f id=4628` |

Or in the browser: **Actions → build-dev → Run workflow**, and pick your branch.

`<id>` is the leading issue number of the branch name, or else the branch name lower-cased
with anything other than letters and digits turned into `-` (max 40 characters). `dev-<id>`
always points at the latest build for that id. The run summary shows the version that was built.
The build uses the latest *pushed* commit, so `just dev-build` refuses to run if your branch has
unpushed commits.

### Releasing (maintainers)

You need push rights to `main`, including the bypass in branch protection, because
`cargo release` pushes a commit and a tag straight to `main`. The release recipes check that
you're on a clean, up-to-date `main`. Each one is a **dry run** unless you add `--execute`, so
run it once without, read the output, then run it again with `--execute`.

| Step | Command | Runs |
|---|---|---|
| Cut an RC (optional, repeatable) | `just rc --execute` | `cargo release rc --execute` |
| Cut the release | `just release --execute` | `cargo release release --execute` |
| Start the next cycle | `just next-version 27.1.0 --execute` | `cargo release version 27.1.0-dev --execute`<br>`git commit -am "chore: start 27.1.0 development"`<br>`git push origin HEAD` |

For example: `26.12.0-dev` → `just rc` → `26.12.0-rc.1` → `just rc` → `26.12.0-rc.2` →
`just release` → `26.12.0` → `just next-version 27.1.0` → `27.1.0-dev`.

Pushing the tag makes CI check it, create the GitHub release (a pre-release for RCs), add the
schema notes and build the images. Run `just next-version` straight after every release.
Otherwise `Cargo.toml` still says `26.12.0` and the next RC fails, because `cargo release` refuses
to go from `26.12.0` to `26.12.0-rc.1` (semver treats that as a downgrade).


## GraphQL schema tracking

Changes to Rust types can change the public GraphQL API: fields can disappear, argument types can change, and new fields can become available. Schema tracking makes API changes visible during review and provides schema release notes.

The idea is to keep a readable API snapshot alongside the Rust code. Before every commit, a local hook generates `schema.graphql` from the Platform Rust types and includes the snapshot in the commit. GitHub CI compares committed snapshots with GraphQL Inspector and reports breaking, potentially dangerous, and non-breaking
changes in PRs. Each release gets schema release notes generated from the
previous and current release snapshots.

Schema reports are informational and require no approval.

Schema tracking covers `product-platform`.

### Set up

Install the Rust toolchain described under Development, then install the commit hook with
`just setup`, or by hand:

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

Pushing a release tag (see [Versioning and releases](#versioning-and-releases)) creates the
GitHub release and triggers a comparison between the saved schemas in the
previous and new release tags. Stable releases compare with the previous published
stable release. Prereleases compare with the previous published release, including
prereleases. Release publication timestamps determine the previous release.

CI adds the schema report to the release description between the GraphQL schema
markers, preserving surrounding release text. The job summary and artifact also
contain the report. Schema reporting runs independently of release image builds.
The first release reports the absence of a previous release; missing historical
snapshots produce an unavailable-comparison report.
