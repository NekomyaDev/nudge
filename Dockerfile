FROM python:3.12-slim

LABEL maintainer="NekomyaDev <elaport0880@gmail.com>"
LABEL description="Nudge - Typed, replayable, budget-aware programming language for LLM agents"
LABEL version="1.2.0"

# Install Node.js for TypeScript backend
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl ca-certificates && \
    curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Install Nudge
ARG NUDGE_VERSION=v1.2.0
RUN curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Set working directory
WORKDIR /workspace

# Default command
CMD ["nudgec", "--help"]
