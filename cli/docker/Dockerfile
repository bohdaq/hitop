# ghcr.io/bohdaq/hitop: the static (musl) hitop binary on a distroless base.
# Built by .github/workflows/release-cli.yml from prebuilt binaries in <arch>/hitop.
#   docker run --rm -v "$PWD:/work" ghcr.io/bohdaq/hitop run ./my-api
FROM gcr.io/distroless/static-debian12:nonroot
ARG TARGETARCH
COPY ${TARGETARCH}/hitop /usr/local/bin/hitop
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/hitop"]
