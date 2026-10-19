# syntax=docker/dockerfile:1
FROM rust:1.94-slim-bookworm AS build
ARG PRODUCT=platform
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
ENV CARGO_PROFILE_RELEASE_DEBUG=false \
    CARGO_PROFILE_RELEASE_STRIP=symbols
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target,id=platform-api-target-${PRODUCT} \
    cargo build --release --locked --features product-${PRODUCT} \
 && cp target/release/platform-api /platform-api

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /platform-api /platform-api
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/platform-api"]
