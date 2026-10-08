# One image, one server (ADR-0030, #406): `algo-synth serve` serves the built
# app and, under /api, the assistant (ADR-0028). The server does no audio work
# (ADR-0001); the browser plays.
# Build: make image · Run: make serve

# Rust builds the engine to wasm and the server, in one toolchain.
FROM docker.io/library/rust:1-trixie AS rust
WORKDIR /src
# Install the rust-toolchain.toml toolchain (with its wasm target) in its own
# cached layer; otherwise rustup auto-installs it inside the cargo build layer
# on every crates/ change, adding ~1 GB per build.
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# The assistant's system prompt includes the language (#383) at build time.
COPY .openspec/language.md ./.openspec/language.md
# The commit the wasm is built from (#197). Declared here, after the copies, so a new commit
# also rebuilds this layer.
ARG ALGO_BUILD_SHA=
ENV ALGO_BUILD_SHA=$ALGO_BUILD_SHA
RUN cargo build --locked --release --target wasm32-unknown-unknown -p algo-dsp \
 && cargo build --locked --release -p algo-assist

FROM docker.io/library/node:24 AS web
WORKDIR /src/web
# The page reads its version from the workspace Cargo.toml and shows the same commit (#197).
COPY Cargo.toml /src/Cargo.toml
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
COPY --from=rust /src/target/wasm32-unknown-unknown/release/algo_dsp.wasm ./public/dsp.wasm
# Last, so a new commit does not redo npm ci.
ARG ALGO_BUILD_SHA=
ENV ALGO_BUILD_SHA=$ALGO_BUILD_SHA
RUN npm run build

FROM docker.io/library/debian:trixie-slim
COPY --from=rust /src/target/release/algo-synth /usr/local/bin/algo-synth
COPY --from=web /src/web/dist /srv/algo-synth
# Inside the container it listens on all its interfaces; `make serve`
# publishes the one port on the host's 127.0.0.1 only (ADR-0028).
ENV ALGO_BIND=0.0.0.0:6340 ALGO_WEB=/srv/algo-synth
EXPOSE 6340
USER 65534
CMD ["algo-synth", "serve"]
