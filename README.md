# Open Targets nextgen Platform API 

TBD

## Development

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


## Export the GraphQL schema

Download the schema into `graphql.schema` from `https://api.platform.opentargets.xyz/latest/graphql/schema`:

```bash
curl --fail \
  https://api.platform.opentargets.xyz/latest/graphql/schema \
  --output graphql.schema
```

This saves the initial schema comparison baseline.

Local Rust schema can be exported with:
```bash
cargo run --locked --example export-schema --features product-platform > graphql.schema
```

## Set up pre-commit

Install pre-commit:
```bash
uv tool install pre-commit
```

Install the Git hook once per checkout, then check the current working tree:

```bash
pre-commit install
pre-commit run graphql-schema --all-files
```

The hook installs a pinned GraphQL Inspector CLI in its own Node environment.
It exports the Platform schema without starting the server or connecting to databases,
then compares it with `graphql.schema` from `HEAD`. If that baseline does not exist,
it downloads the schema from the new API deployment above.

When the generated snapshot changes, the hook updates `graphql.schema` and stops
the commit so you can review and stage the file. All schema changes, including
breaking changes, are advisory: double-check that they are intentional, then stage
`graphql.schema` and retry the commit. The report is shown again, but breaking
changes do not block the retry. Export, download, or comparison errors still fail
the check. During commits, pre-commit
temporarily hides unstaged tracked changes. The manual `--all-files` command checks
the working tree instead. Stage any new Rust source files before committing.

## GraphQL schema checks in CI

CI regenerates the Platform schema and requires it to match the committed
`graphql.schema`. Breaking changes remain advisory and are shown with dangerous
and non-breaking changes in the job summary and the downloadable
`graphql-schema-release-notes` Markdown artifact.

For PRs into `master`, the comparison uses the PR event's master revision and the
PR merge result, covering the combined change across all PR commits. On pushes to
`master`, it uses master before the push, so the report still shows the changes
after merging. Manual runs and PRs into other branches compare with fetched master.
The exact baseline and compared commit IDs are recorded in the notes.

These are release-note fragments relative to master, generated before that baseline
moves; they are not tag-to-tag release history. They can be included in release
notes, but CI does not publish or modify GitHub releases. If repository doesn't have
`graphql.schema`, CI compares against `https://api.platform.opentargets.xyz/latest/graphql/schema`.  Download or schema parsing failures still fail CI.
