# syntax=docker/dockerfile:1.7
FROM rust:1.95-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
RUN cargo build --release -p save-server --bin mh-save-server

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=build /src/target/release/mh-save-server /app/mh-save-server
ENV MH_SAVE_SYNC_BIND=0.0.0.0:8080
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/app/mh-save-server"]
