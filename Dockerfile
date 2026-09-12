# syntax=docker/dockerfile:1.7
# Build one OCI image for both Docker and Podman/Buildah consumers.
# The default binary is dependency-free; richer adapters can be selected with
# --build-arg BINARY=worker-http --build-arg CARGO_FEATURES=http.
ARG BUILDPLATFORM
ARG TARGETPLATFORM
FROM --platform=$BUILDPLATFORM rust:1.88-bookworm AS builder
ARG TARGETARCH
ARG BINARY=oci-http
ARG CARGO_FEATURES=
WORKDIR /src
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu
COPY . .
RUN set -eux; \
    case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-gnu; linker=x86_64-linux-gnu-gcc ;; \
      arm64) target=aarch64-unknown-linux-gnu; linker=aarch64-linux-gnu-gcc ;; \
      *) echo "unsupported target architecture: $TARGETARCH" >&2; exit 2 ;; \
    esac; \
    rustup target add "$target"; \
    if [ -n "$CARGO_FEATURES" ]; then \
      env "CARGO_TARGET_$(printf '%s' "$target" | tr '[:lower:]-' '[:upper:]_')_LINKER=$linker" \
        cargo build --release --target "$target" --bin "$BINARY" --features "$CARGO_FEATURES"; \
    else \
      env "CARGO_TARGET_$(printf '%s' "$target" | tr '[:lower:]-' '[:upper:]_')_LINKER=$linker" \
        cargo build --release --target "$target" --bin "$BINARY"; \
    fi; \
    cp "target/$target/release/$BINARY" /out/lambda

FROM --platform=$TARGETPLATFORM debian:bookworm-slim AS runtime
ARG SOURCE_REPOSITORY=https://github.com/happy-wakey/happy-wakey-lambdas
ARG VCS_REF=unknown
LABEL org.opencontainers.image.source="$SOURCE_REPOSITORY" \
      org.opencontainers.image.revision="$VCS_REF" \
      org.opencontainers.image.title="happy-wakey-lambdas" \
      org.opencontainers.image.description="Provider-neutral lambda OCI runtime"
RUN groupadd --system lambda \
    && useradd --system --gid lambda --home-dir /nonexistent --no-create-home lambda
COPY --from=builder /out/lambda /usr/local/bin/lambda
COPY entrypoint.sh /entrypoint.sh
RUN chmod 0555 /usr/local/bin/lambda /entrypoint.sh
USER lambda
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["/entrypoint.sh"]
CMD ["/usr/local/bin/lambda"]
