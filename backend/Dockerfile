# syntax=docker/dockerfile:1
# Build context: backend/ (the prompts and migrations are embedded in the binary
# at compile time through include_str! and sqlx::migrate!, so both directories
# must be in the context but nothing else is needed at runtime).

# Keep in step with rust-toolchain.toml.
FROM rust:1.100.0-bookworm AS build
WORKDIR /src
# cmake: the `opus` crate builds libopus from source and links it statically,
# so the runtime image needs no audio library (docs/adr/0013).
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*
COPY . .
# Cache mounts keep the crate registry and the compiled dependencies between
# builds; the target dir is a cache, so the binary is copied out in the same step.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p api --bin teach-me \
    && install -D target/release/teach-me /out/teach-me

# PDFium, the engine that reads uploaded PDFs (docs: docs/adr/0007-pdf-extraction.md).
# Prebuilt shared library from bblanchon/pdfium-binaries, pinned by release and
# checked by hash; it needs nothing but glibc. To update: pick a release, copy
# the sha256 of each tarball from its page, and run the PDF tests against it.
FROM debian:bookworm-slim AS pdfium
ARG TARGETARCH
ARG PDFIUM_RELEASE=8138
ARG PDFIUM_SHA256_AMD64=d6b1e28950a0392590d79bbdbb7eb19686872b688f8a36f170acb19e41473113
ARG PDFIUM_SHA256_ARM64=c70b1ce4d13257f741af19e0aeee575890875995eb81370a65be942e601a1862
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && case "$TARGETARCH" in \
        amd64) arch=x64; sha="$PDFIUM_SHA256_AMD64" ;; \
        arm64) arch=arm64; sha="$PDFIUM_SHA256_ARM64" ;; \
        *) echo "no PDFium build for $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && curl -fsSL -o /tmp/pdfium.tgz \
        "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_RELEASE}/pdfium-linux-${arch}.tgz" \
    && echo "${sha}  /tmp/pdfium.tgz" | sha256sum -c - \
    && mkdir /pdfium \
    && tar -xzf /tmp/pdfium.tgz -C /pdfium lib/libpdfium.so LICENSE licenses

FROM debian:bookworm-slim
# curl: HEALTHCHECK. sqlite3: hot backups through `docker compose exec` (deploy/backup.sh).
# ca-certificates: TLS towards the LLM provider.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl sqlite3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin teachme \
    && mkdir /data /audio && chown teachme:teachme /data /audio
COPY --from=build /out/teach-me /usr/local/bin/teach-me
# Loaded at run time by the backend (TEACH_ME_PDFIUM_DIR); licenses kept next to it.
COPY --from=pdfium /pdfium/lib/libpdfium.so /usr/local/lib/pdfium/libpdfium.so
COPY --from=pdfium /pdfium/LICENSE /pdfium/licenses /usr/share/doc/pdfium/
# Versioned config without secrets, baked in so no bind mount depends on where the build runs.
COPY deploy/teach_me.toml /etc/teach_me/teach_me.toml

USER teachme
WORKDIR /data
# Inside the container the server must listen on all interfaces; what is
# reachable from outside is decided by the port mapping in docker-compose.yml.
ENV TEACH_ME_PDFIUM_DIR=/usr/local/lib/pdfium \
    TEACH_ME_CONFIG=/etc/teach_me/teach_me.toml \
    TEACH_ME_BIND_ADDR=0.0.0.0:8080 \
    TEACH_ME_DATABASE_URL=sqlite:///data/teach_me.db \
    TEACH_ME_AUDIO_DIR=/audio \
    TEACH_ME_LOG_FORMAT=json
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health || exit 1
ENTRYPOINT ["teach-me"]
