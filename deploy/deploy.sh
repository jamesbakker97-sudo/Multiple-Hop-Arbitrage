#!/usr/bin/env bash
set -euo pipefail
# Build image and launch the control-plane using the production compose
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "Building control-plane image..."
docker build -t control-plane:prod "$ROOT_DIR/control-plane"

echo "Starting services with docker-compose..."
docker-compose -f "$ROOT_DIR/control-plane/docker-compose.prod.yml" up -d --build

echo "Done. Use 'docker-compose -f $ROOT_DIR/control-plane/docker-compose.prod.yml logs -f' to follow logs."
