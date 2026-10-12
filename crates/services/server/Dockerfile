# Stage 1: Build
FROM rust:bookworm AS builder
WORKDIR /app

# Fix for the tonic build error & webrtc-sys clang requirement: install protobuf-compiler and clang-21
RUN apt-get update && apt-get install -y wget gnupg lsb-release protobuf-compiler pkg-config libssl-dev libglib2.0-dev software-properties-common && \
    wget https://apt.llvm.org/llvm.sh && \
    chmod +x llvm.sh && \
    ./llvm.sh 21 && \
    apt-get install -y clang-21 clang++-21 && \
    update-alternatives --install /usr/bin/clang clang /usr/bin/clang-21 100 && \
    update-alternatives --install /usr/bin/clang++ clang++ /usr/bin/clang++-21 100 && \
    rm -rf /var/lib/apt/lists/* llvm.sh

ENV CC=clang-21
ENV CXX=clang++-21
ENV CARGO_BUILD_JOBS=4
ENV CARGO_HTTP_MULTIPLEXING=false
ENV CARGO_HTTP_TIMEOUT=120

# Copy the entire workspace
COPY . .

# Build the backend server
RUN cargo build --release --bin server

# Stage 2: Runner
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

RUN groupadd -r -g 999 voiceapp && useradd -r -u 999 -g voiceapp voiceapp
RUN mkdir -p /app /data && chown -R voiceapp:voiceapp /app /data

WORKDIR /app

COPY --from=builder /app/target/release/server /app/server
RUN chown voiceapp:voiceapp /app/server && chmod +x /app/server

USER 999

EXPOSE 4000
CMD ["/app/server"]
