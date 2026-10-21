#!/bin/bash
# SPDX-License-Identifier: MIT
set -euo pipefail
cd /downloads
echo '87363c24f10e06321eb362765521094e94471f6bff8bce2d6b3e40f39f004953  linux-7.3.tar.xz' | sha256sum -c -
if [[ ! -d /build/linux-7.3 ]]; then
    tar -xJf linux-7.3.tar.xz -C /build
fi
bash /input/build-linux.sh /build/linux-7.3 /build/kernel
cp /build/kernel/arch/arm64/boot/Image /out/Image
cp /build/kernel/.config /out/linux.config
cp /build-packages.tsv /out/build-packages.tsv
tar -C /build/kernel/modules-root -czf /out/modules.tar.gz lib
cd /out
sha256sum Image linux.config modules.tar.gz > SHA256SUMS
