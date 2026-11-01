# hitop sync server image. Build from the repository root or sync-server/:
#   docker build -t hitop-sync sync-server
# Run (data in a volume; put a TLS proxy in front, see README.md):
#   docker run -p 7881:7881 -v hitop-sync:/data -e HITOP_SYNC_PUBLIC_URL=https://sync.example.com \
#     -e SMTP_URL=smtp://user:pass@mail.example.com:587 hitop-sync
FROM rust:1.99-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked --bin hitop-sync-server && mkdir /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/hitop-sync-server /usr/local/bin/hitop-sync-server
COPY LICENSE /usr/share/doc/hitop-sync/LICENSE
# A data directory the nonroot user (65532) owns, so a fresh volume is writable.
COPY --from=build --chown=65532:65532 /data /data
ENV RWS_CONFIG_IP=0.0.0.0 \
    RWS_CONFIG_PORT=7881 \
    RWS_CONFIG_LOG_FORMAT=off \
    RWS_CONFIG_MAX_BODY_SIZE_IN_BYTES=6291456 \
    HITOP_SYNC_DATABASE=sqlite:/data/sync.db
VOLUME /data
WORKDIR /data
EXPOSE 7881
USER nonroot
ENTRYPOINT ["/usr/local/bin/hitop-sync-server"]
