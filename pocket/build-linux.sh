#!/bin/bash
# SPDX-License-Identifier: MIT
set -euo pipefail
[[ $# -eq 2 ]] || { echo 'usage: pocket/build-linux.sh LINUX_SOURCE BUILD_DIR' >&2; exit 2; }
[[ $(uname -s) == Linux ]] || { echo 'Build inside an ARM64 Linux build environment.' >&2; exit 2; }
source_dir=$(cd "$1" && pwd)
mkdir -p "$2"
build_dir=$(cd "$2" && pwd)
pocket_dir=$(cd "$(dirname "$0")" && pwd)
test "$(make -s -C "$source_dir" kernelversion)" = 7.3.0
export ARCH=arm64
export KBUILD_BUILD_TIMESTAMP='2026-10-18 00:00:00 +0000'
export KBUILD_BUILD_USER=pocketagent
export KBUILD_BUILD_HOST=builder
make -C "$source_dir" O="$build_dir" defconfig
"$source_dir/scripts/kconfig/merge_config.sh" -m -O "$build_dir" \
    "$build_dir/.config" "$pocket_dir/linux-arm64.config"
make -C "$source_dir" O="$build_dir" olddefconfig
# Unsupported/renamed options must be reviewed; do not fabricate config symbols.
for option in VIRTIO_MMIO VIRTIO_BLK VIRTIO_NET VIRTIO_CONSOLE SERIAL_AMBA_PL011_CONSOLE EXT4_FS; do
    grep -qx "CONFIG_$option=y" "$build_dir/.config"
done
make -C "$source_dir" O="$build_dir" -j "${POCKET_BUILD_JOBS:-8}" Image modules
make -C "$source_dir" O="$build_dir" INSTALL_MOD_PATH="$build_dir/modules-root" modules_install
sha256sum "$build_dir/arch/arm64/boot/Image" "$build_dir/.config"
