# One image, one server (ADR-0030, #406): `algo-synth serve` serves the built
# app and, under /api, the assistant (ADR-0028). The server does no audio work
# (ADR-0001); the browser plays.
# Build: make image · Run: make serve

# Rust builds the engine to wasm and the server, in one toolchain.
FROM docker.io/library/rust:1-trixie AS rust
# Labelled, so `make image` can drop this stage's old images and no one else's.
LABEL algo-synth.stage=rust
WORKDIR /src
# Install the rust-toolchain.toml toolchain (with its wasm target) in its own
# cached layer; otherwise rustup auto-installs it inside the cargo build layer
# on every crates/ change, adding ~1 GB per build.
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
# The build settings: the engine's wasm stack among them (#555), without which it can't start.
COPY .cargo ./.cargo
COPY crates ./crates
# A workspace member (#482): cargo loads the whole workspace, so it must be here.
COPY syntax/song-lsp ./syntax/song-lsp
# The assistant's system prompt includes the language and sound design (#383, #514) at build time.
COPY .openspec/language.md .openspec/sound-design.md ./.openspec/
# The commit the wasm is built from (#197). Declared here, after the copies, so a new commit
# also rebuilds this layer.
ARG ALGO_BUILD_SHA=
ENV ALGO_BUILD_SHA=$ALGO_BUILD_SHA
# target/ and the crate registry live in cache mounts, kept between builds and
# reused, not in this layer: each commit rebuilds this layer, and with target/
# in it every build left a ~2.6 GB image behind until the disk filled. Only the
# two outputs go into the layer, under /out.
RUN --mount=type=cache,id=algo-synth-target,target=/src/target \
    --mount=type=cache,id=algo-synth-cargo,target=/usr/local/cargo/registry \
    cargo build --locked --release --target wasm32-unknown-unknown -p algo-dsp \
 && cargo build --locked --release -p algo-assist \
 && mkdir -p /out \
 && cp target/wasm32-unknown-unknown/release/algo_dsp.wasm target/release/algo-synth /out/

FROM docker.io/library/node:24 AS web
LABEL algo-synth.stage=web
WORKDIR /src/web
# The page reads its version from the workspace Cargo.toml and shows the same commit (#197).
COPY Cargo.toml /src/Cargo.toml
COPY web/package.json web/package-lock.json ./
# npm's downloads in a cache mount, so npm ci stays quick after `make image`
# drops this stage's old images.
RUN --mount=type=cache,id=algo-synth-npm,target=/root/.npm npm ci
COPY web ./
# The example songs and scores the build copies into the app (#20).
COPY examples /src/examples
COPY --from=rust /out/algo_dsp.wasm ./public/dsp.wasm
# Last, so a new commit does not redo npm ci.
ARG ALGO_BUILD_SHA=
ENV ALGO_BUILD_SHA=$ALGO_BUILD_SHA
RUN npm run build

FROM docker.io/library/debian:trixie-slim
COPY --from=rust /out/algo-synth /usr/local/bin/algo-synth
COPY --from=web /src/web/dist /srv/algo-synth
# Inside the container it listens on all its interfaces; `make serve`
# publishes the one port on the host's 127.0.0.1 only (ADR-0028).
ENV ALGO_BIND=0.0.0.0:6340 ALGO_WEB=/srv/algo-synth
EXPOSE 6340
USER 65534
CMD ["algo-synth", "serve"]
