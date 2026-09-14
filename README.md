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

```bash
    cargo build --release
```
