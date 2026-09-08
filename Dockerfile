# One binary, and a scratch of static assets is not even needed: the client is served separately.
#
# The build stage compiles the workspace; the runtime stage carries the executable and nothing
# else that could be exploited. There is no interpreter here, no package manager and no
# application source — which is the deployment story the rewrite was partly for.

FROM rust:1.94-slim-bookworm AS build

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

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 aurum

COPY --from=build /src/target/release/aurum-api /usr/local/bin/aurum-api

USER aurum
WORKDIR /app

ENV DATA_DIR=/app/var/data BIND=0.0.0.0:8080 SIGNAL_BIND=0.0.0.0:8081
EXPOSE 8080 8081

ENTRYPOINT ["aurum-api"]
