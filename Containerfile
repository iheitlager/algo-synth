# Static files (ADR-0006) and, as its own image, the assist server (ADR-0028):
# neither does audio work.
# Build: make image · Run: make serve

FROM docker.io/library/rust:1 AS wasm
WORKDIR /src
# Install the rust-toolchain.toml toolchain (with its wasm target) in its own
# cached layer; otherwise rustup auto-installs it inside the cargo build layer
# on every crates/ change, adding ~1 GB per build.
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# The commit the wasm is built from (#197). Declared here, after the copies, so a new commit
# also rebuilds this layer.
ARG ALGO_BUILD_SHA=
ENV ALGO_BUILD_SHA=$ALGO_BUILD_SHA
RUN cargo build --locked --release --target wasm32-unknown-unknown -p algo-dsp

FROM docker.io/library/node:24 AS web
WORKDIR /src/web
# The page reads its version from the workspace Cargo.toml and shows the same commit (#197).
COPY Cargo.toml /src/Cargo.toml
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
COPY --from=wasm /src/target/wasm32-unknown-unknown/release/algo_dsp.wasm ./public/dsp.wasm
# Last, so a new commit does not redo npm ci.
ARG ALGO_BUILD_SHA=
ENV ALGO_BUILD_SHA=$ALGO_BUILD_SHA
RUN npm run build

# The assist server (ADR-0028, #386): Rust, linking the engine for its song
# tools. `make image` builds it with --target assist; it runs beside Caddy on
# a private network and holds no key until `op run` passes them in.
FROM docker.io/library/rust:1-trixie AS assist-build
WORKDIR /src
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# The system prompt includes the language (#383) at build time.
COPY .openspec/language.md ./.openspec/language.md
RUN cargo build --locked --release -p algo-assist

FROM docker.io/library/debian:trixie-slim AS assist
COPY --from=assist-build /src/target/release/assist /usr/local/bin/assist
# Inside the container it listens on all its interfaces; only Caddy, on the
# same network, reaches it: no port is published on the host.
ENV ASSIST_BIND=0.0.0.0:6342
EXPOSE 6342
USER 65534
CMD ["assist", "serve"]

FROM docker.io/library/caddy:2
COPY Caddyfile /etc/caddy/Caddyfile
COPY --from=web /src/web/dist /usr/share/caddy
