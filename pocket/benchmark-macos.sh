#!/bin/bash
# SPDX-License-Identifier: MIT
# Sequential comparisons; the source must be a stopped performance test disk.
set -euo pipefail
[[ $# == 3 ]] || { echo 'usage: benchmark-macos.sh KERNEL PERFORMANCE_DISK OUTPUT_DIR' >&2; exit 2; }
[[ $(uname -s) == Darwin ]] || { echo 'Requires macOS APFS disk cloning.' >&2; exit 2; }
kernel=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
disk=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
test ! -e "$3"
mkdir -p "$3"
out=$(cd "$3" && pwd)
source_dir=$(cd "$(dirname "$0")/.." && pwd)
vm="$source_dir/pocket/runtime/target/release/pocket-vm"
test -x "$vm"
for profile in default-cache tb256 tb128; do
    cp -c "$disk" "$out/$profile.ext4"
    if [[ $profile != default-cache ]]; then
        POCKET_VM_CPU=cortex-a72 POCKET_VM_CPUS=1 POCKET_VM_TB_MIB=${profile#tb} \
            "$vm" "$out/$profile" "$kernel" "$out/$profile.ext4" > "$out/$profile.log" 2>&1
    else
        env -u POCKET_VM_TB_MIB POCKET_VM_CPU=cortex-a72 POCKET_VM_CPUS=1 \
            "$vm" "$out/$profile" "$kernel" "$out/$profile.ext4" > "$out/$profile.log" 2>&1
    fi
    echo "$profile: passed; report in $out/$profile/report.json"
done
