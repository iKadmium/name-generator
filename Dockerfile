# syntax=docker/dockerfile:1

# Always builds on the native build platform (no QEMU) and cross-compiles a
# static musl binary for the target platform using Rust's bundled rust-lld,
# then ships it in an empty image. No OS packages are needed because the crate
# has no C dependencies.

FROM --platform=$BUILDPLATFORM rust:slim AS build
ARG TARGETARCH
RUN case "$TARGETARCH" in \
      amd64) echo x86_64-unknown-linux-musl ;; \
      arm64) echo aarch64-unknown-linux-musl ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac > /target-triple \
    && rustup target add "$(cat /target-triple)"
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,id=target-$TARGETARCH \
    target="$(cat /target-triple)" \
    && cargo build --release --locked --target "$target" \
    && cp "target/$target/release/name-generator" /name-generator

FROM scratch
COPY --from=build /name-generator /name-generator
USER 65534:65534
EXPOSE 3000
ENTRYPOINT ["/name-generator"]
