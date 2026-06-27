#!/usr/bin/env bash
# Multi-architecture Docker build for Portail
# Builds and pushes images for linux/amd64 and linux/arm64

set -euo pipefail

IMAGE_NAME="${IMAGE_NAME:-portail}"
VERSION="${VERSION:-2.1.0}"
REGISTRY="${REGISTRY:-ghcr.io/peterlodri-sec}"

echo "=== Building ${IMAGE_NAME}:${VERSION} for multiple architectures ==="

# Check if docker buildx is available
if ! command -v docker &> /dev/null; then
    echo "Error: docker not found"
    exit 1
fi

if ! docker buildx version &> /dev/null; then
    echo "Error: docker buildx not available"
    exit 1
fi

# Create buildx builder if it doesn't exist
if ! docker buildx inspect portail-builder &> /dev/null; then
    echo "Creating buildx builder..."
    docker buildx create --name portail-builder --use
fi

# Ensure builder is running
docker buildx use portail-builder

# Drive the builder (required for multi-arch builds)
docker buildx inspect --bootstrap

# Build and push for both architectures
echo "Building for linux/amd64 and linux/arm64..."
docker buildx build \
    --platform linux/amd64,linux/arm64 \
    --tag "${REGISTRY}/${IMAGE_NAME}:${VERSION}" \
    --tag "${REGISTRY}/${IMAGE_NAME}:latest" \
    --push \
    --build-arg VERSION="${VERSION}" \
    .

# Clean up builder (optional)
# docker buildx rm portail-builder

echo "=== Successfully built and pushed ${IMAGE_NAME}:${VERSION} ==="
echo "Images available:"
echo "  - ${REGISTRY}/${IMAGE_NAME}:${VERSION}"
echo "  - ${REGISTRY}/${IMAGE_NAME}:latest"
