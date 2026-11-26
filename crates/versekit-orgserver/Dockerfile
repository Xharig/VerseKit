# Template server of the VerseKit org standard (versekit-org v1).
# SPDX-License-Identifier: GPL-3.0-only
#
# Built from the root of the repository:
#   docker build -f crates/versekit-orgserver/Dockerfile -t versekit-orgserver .
# Started best with tools/orga-standard/docker-compose.yml (or docker-compose.caddy.yml with HTTPS included).
# /data holds state.json and the generated admin password; config.json lies there too or is mounted read-only
# (VERSEKIT_ORG_CONFIG). The server speaks plain HTTP on one port; a reverse proxy in front provides HTTPS.
# crates/versekit-orgserver/container_check.sh starts the image and checks login and state across a restart.
#
# Hardened: a small base without a shell or package manager (distroless), never root (user nonroot, 65532),
# one port, a health check through the server itself. The compose files add read_only, cap_drop ALL and
# no-new-privileges.

# Both bases are pinned to the digest of their multi-arch index (amd64 and arm64), the tag only names it.
FROM rust:1.100-bookworm@sha256:75d26c23c24cc6ea1cdabf0714a6bff8fd7bf3248716043fd7ae86dac7f41b94 AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p versekit-orgserver && mkdir /data-empty

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
COPY --from=build /src/target/release/versekit-orgserver /usr/local/bin/versekit-orgserver
COPY --from=build --chown=nonroot:nonroot /data-empty /data
USER nonroot:nonroot
VOLUME /data
EXPOSE 8080
ENV VERSEKIT_ORG_CONFIG=/data/config.json \
    VERSEKIT_ORG_STATE=/data/state.json \
    VERSEKIT_ORG_LISTEN=0.0.0.0:8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/versekit-orgserver", "--healthcheck"]
ENTRYPOINT ["/usr/local/bin/versekit-orgserver"]
