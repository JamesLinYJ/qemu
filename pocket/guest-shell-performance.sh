#!/bin/bash
# SPDX-License-Identifier: MIT
# Guest-only latency measurements in a disposable validation disk.
set -euo pipefail
finish() {
    result=$?
    if [[ $result -ne 0 ]]; then echo "POCKET_ACCEPTANCE_FAIL:$result"; fi
    sync
    systemctl --no-block poweroff
}
trap finish EXIT
export LC_ALL=C.UTF-8
echo "POCKET_KERNEL:$(uname -r)"
[[ $(uname -r) == 7.3.0-pocket ]]
[[ $(cat /proc/1/comm) == systemd ]]
/usr/local/libexec/pocket-linux-bench
echo POCKET_ACCEPTANCE_PASS
sleep 1
