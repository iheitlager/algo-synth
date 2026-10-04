# Static files only (ADR-0006): the server does no audio work.
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
RUN cargo build --locked --release --target wasm32-unknown-unknown -p algo-dsp

FROM docker.io/library/node:24 AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
COPY --from=wasm /src/target/wasm32-unknown-unknown/release/algo_dsp.wasm ./public/dsp.wasm
RUN npm run build

FROM docker.io/library/caddy:2
COPY Caddyfile /etc/caddy/Caddyfile
COPY --from=web /src/web/dist /usr/share/caddy
