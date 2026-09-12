# One binary and a directory of static assets — acceptance criterion 7 of the rewrite.
#
# Two build stages, because the client and the server are the same workspace compiled for two
# targets: `aurum-api` natively, `aurum-web` to WebAssembly. The runtime stage carries the
# executable and the distribution and nothing else. There is no interpreter here, no package
# manager and no application source.

FROM rust:1.94-slim-bookworm AS server

WORKDIR /src

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Dependencies first, so an edit to our own code does not rebuild the world.
COPY Cargo.toml Cargo.lock ./
COPY crates/core/Cargo.toml crates/core/
COPY crates/api/Cargo.toml crates/api/
COPY crates/web/Cargo.toml crates/web/
RUN mkdir -p crates/core/src crates/api/src crates/web/src \
 && echo 'pub fn placeholder() {}' > crates/core/src/lib.rs \
 && echo 'fn main() {}' > crates/api/src/main.rs \
 && echo 'fn main() {}' > crates/web/src/main.rs \
 && cargo build --release -p aurum-api \
 && rm -rf crates

COPY migrations migrations
COPY crates crates
# Touch, so the placeholder build above does not look newer than the real source.
RUN find crates -name '*.rs' -exec touch {} + \
 && cargo build --release -p aurum-api

FROM rust:1.94-slim-bookworm AS client

WORKDIR /src

# Node is here for three things, none of them application logic: Tailwind reads the Rust for the
# class names to emit, Workbox writes the service worker over the distribution Trunk has just
# produced, and Pdfium is copied out of node_modules rather than committed.
#
# Node comes from the official image, not Debian's apt package: bookworm pins Node 18, and
# @tailwindcss/oxide needs >= 20 to install its native platform binary — under 18 the install
# silently skips it, so `trunk build` fails at PreBuild with "Cannot find native binding".
COPY --from=node:22-bookworm-slim /usr/local/bin/node /usr/local/bin/node
COPY --from=node:22-bookworm-slim /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && ln -s /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
 && apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/* \
 && rustup target add wasm32-unknown-unknown \
 && cargo install --locked trunk@0.21.14

COPY web/package.json web/package-lock.json web/
RUN cd web && npm ci

COPY Cargo.toml Cargo.lock Trunk.toml ./
COPY crates crates
COPY web web

# Trunk reads the pinned tool versions out of Trunk.toml, fetches them once, then builds the
# crate to WebAssembly, hashes the assets, and runs the service-worker hook over the result.
RUN trunk build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 aurum

COPY --from=server /src/target/release/aurum-api /usr/local/bin/aurum-api
COPY --from=client /src/crates/web/dist /app/web

# Owned by aurum before the volume mount claims it, so the SQLite file it holds is writable by
# the user the process actually runs as.
RUN mkdir -p /app/var/data && chown -R aurum:aurum /app/var

USER aurum
WORKDIR /app

ENV DATA_DIR=/app/var/data BIND=0.0.0.0:8080 SIGNAL_BIND=0.0.0.0:8081 WEB_DIR=/app/web
EXPOSE 8080 8081

ENTRYPOINT ["aurum-api"]
