# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# Étape de build : compilation d'un binaire statique musl.
# L'image rust:alpine cible nativement le triplet musl, donc le binaire produit
# est statiquement lié et peut tourner dans une image « scratch ».
# ---------------------------------------------------------------------------
FROM rust:1.95-alpine AS builder

# Dépendances de build : aws-lc-rs (backend TLS de rustls) nécessite un
# compilateur C, cmake et perl ; nasm sert pour l'assembleur optimisé sur x86_64.
RUN apk add --no-cache build-base cmake perl clang nasm

WORKDIR /app

# Cache des dépendances : on compile d'abord un projet factice.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Build réel.
COPY src ./src
RUN touch src/main.rs && cargo build --release \
    && strip target/release/bb_exporter

# ---------------------------------------------------------------------------
# Image finale : entièrement vide (« from scratch »).
# Aucun certificat CA n'est requis car la Bbox présente un certificat
# auto-signé que l'exporteur accepte explicitement.
# ---------------------------------------------------------------------------
FROM scratch

COPY --from=builder /app/target/release/bb_exporter /bb_exporter

EXPOSE 9100
ENTRYPOINT ["/bb_exporter"]
