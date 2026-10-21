# SPDX-License-Identifier: MIT
FROM debian:trixie-slim@sha256:a66b19aa552f595b4ab276c6b4f9b24fa4546632c5ecd88cccdced4a6866323a
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential bc bison flex libssl-dev libelf-dev pkg-config kmod rsync \
    cpio xz-utils ca-certificates curl e2fsprogs file python3 \
    && dpkg-query -W > /build-packages.tsv \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
