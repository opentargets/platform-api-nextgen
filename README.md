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
