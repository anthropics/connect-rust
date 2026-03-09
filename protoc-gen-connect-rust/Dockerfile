# syntax=docker/dockerfile:1.12
FROM --platform=$BUILDPLATFORM rust:1.84-bookworm AS build

# Install musl toolchain for static linking
RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl

# Copy workspace source
WORKDIR /src
COPY . .

# Build the plugin binary (statically linked via musl)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --target x86_64-unknown-linux-musl -p protoc-gen-connect-rust \
 && cp /src/target/x86_64-unknown-linux-musl/release/protoc-gen-connect-rust /usr/local/bin/

FROM scratch
COPY --from=build --link --chown=root:root /etc/passwd /etc/passwd
COPY --from=build --link /usr/local/bin/protoc-gen-connect-rust /
USER nobody
ENTRYPOINT [ "/protoc-gen-connect-rust" ]
