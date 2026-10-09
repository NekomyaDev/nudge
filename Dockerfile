FROM rust:1-slim-bookworm AS compiler

# Build nudgec from this repository's source
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p nudgec

FROM python:3.15-rc-alpine3.22 AS builder

LABEL maintainer="NekomyaDev"
LABEL description="Nudge - Typed, replayable, budget-aware programming language for LLM agents"
LABEL version="1.2.1"
LABEL org.opencontainers.image.source="https://github.com/NekomyaDev/nudge"
LABEL org.opencontainers.image.description="Typed, replayable, budget-aware programming language for LLM agents"
LABEL org.opencontainers.image.licenses="Apache-2.0"
LABEL org.opencontainers.image.vendor="NekomyaDev"
LABEL org.opencontainers.image.title="Nudge"

# Install Node.js for TypeScript backend
RUN apt-get update && \
    apt-get upgrade -y && \
    apt-get install -y --no-install-recommends curl ca-certificates gnupg && \
    curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    npm install -g npm@latest && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Final stage
FROM python:3.15-rc-alpine3.22

# Copy only necessary files from builder stages
COPY --from=compiler /src/target/release/nudgec /usr/local/bin/nudgec
COPY --from=builder /usr/bin/node /usr/bin/node
COPY --from=builder /usr/lib/node_modules /usr/lib/node_modules

# Python runtime from this repository's source
COPY runtime /opt/runtime
RUN pip install --no-cache-dir /opt/runtime && rm -rf /opt/runtime

# Update packages and fix vulnerabilities
RUN apt-get update && \
    apt-get upgrade -y && \
    pip install --no-cache-dir --upgrade setuptools msgpack && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /workspace

# Default command
CMD ["nudgec", "--help"]
