# syntax=docker/dockerfile:1
FROM rust:1.94-slim-trixie AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
ARG PRODUCT=platform
ENV CARGO_PROFILE_RELEASE_DEBUG=false \
    CARGO_PROFILE_RELEASE_STRIP=symbols
COPY --from=planner /src/recipe.json recipe.json
# Dependencies only: this layer is cached until Cargo.toml/Cargo.lock change
RUN cargo chef cook --release --locked --features product-${PRODUCT} --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --features product-${PRODUCT} \
 && cp target/release/platform-api /platform-api

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /platform-api /platform-api
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/platform-api"]
