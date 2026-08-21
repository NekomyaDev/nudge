FROM python:3.13-slim AS builder

LABEL maintainer="NekomyaDev <elaport0880@gmail.com>"
LABEL description="Nudge - Typed, replayable, budget-aware programming language for LLM agents"
LABEL version="1.2.0"
LABEL org.opencontainers.image.source="https://github.com/NekomyaDev/nudge"
LABEL org.opencontainers.image.description="Typed, replayable, budget-aware programming language for LLM agents"
LABEL org.opencontainers.image.licenses="Proprietary"
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

# Install Nudge
ARG NUDGE_VERSION=v1.2.0
RUN curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Final stage
FROM python:3.13-slim

# Copy only necessary files from builder
COPY --from=builder /usr/local/bin/nudgec /usr/local/bin/nudgec
COPY --from=builder /usr/bin/node /usr/bin/node
COPY --from=builder /usr/lib/node_modules /usr/lib/node_modules

# Copy runtime
COPY runtime/nudge_runtime /usr/local/lib/python3.13/site-packages/nudge_runtime
ENV PYTHONPATH=/usr/local/lib/python3.13/site-packages

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
