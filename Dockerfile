FROM python:3.12-slim

# Install Node.js for TypeScript backend
RUN apt-get update && apt-get install -y curl && \
    curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install Nudge
ARG NUDGE_VERSION=v1.2.0
RUN curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Set working directory
WORKDIR /workspace

# Default command
CMD ["nudgec", "--help"]
