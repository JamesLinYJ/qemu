#!/bin/bash
# SPDX-License-Identifier: MIT
# Offline, repeatable component workload. No package updates or remote servers.
# Substitute this script only in disposable acceptance images.
set -euo pipefail
finish() {
    result=$?
    if [[ $result -ne 0 ]]; then echo "POCKET_ACCEPTANCE_FAIL:$result"; fi
    sync
    systemctl --no-block poweroff
}
trap finish EXIT
export LC_ALL=C.UTF-8
TIMEFORMAT='POCKET_CPU:wall=%3R:user=%3U:system=%3S'
step() {
    name=$1; shift
    echo "POCKET_STEP_BEGIN:$name:$(date +%s%N)"
    time "$@"
    echo "POCKET_STEP_PASS:$name:$(date +%s%N)"
}
echo "POCKET_KERNEL:$(uname -r)"
[[ $(uname -r) == 7.3.0-pocket ]]
[[ $(cat /proc/1/comm) == systemd ]]
systemd-analyze --no-pager blame
systemctl show systemd-networkd-wait-online.service systemd-udev-trigger.service \
    -p Id -p ExecMainStartTimestampMonotonic -p ExecMainExitTimestampMonotonic
for round in 1 2 3; do
    step "python-$round" python3 -c 'import threading,subprocess,ssl; t=threading.Thread(target=lambda: None); t.start(); t.join(); assert subprocess.check_output(["/bin/sh","-c","printf 42"])==b"42"'
    step "node-$round" node -e 'const {Worker}=require("node:worker_threads");const w=new Worker("require(\"node:worker_threads\").parentPort.postMessage(42)",{eval:true});w.on("message",m=>{if(m!==42)process.exitCode=1})'
    step "npm-$round" npm --version
    step "git-$round" git --version
done
step python-extensions python3 -c 'import numpy,pandas,PIL,docx,openpyxl,reportlab,pypdf'
step filesystem bash -c 'd=$(mktemp -d); trap '\''rm -rf "$d"'\'' EXIT; for i in {1..100}; do printf data > "$d/$i"; done; find "$d" -type f | wc -l; tar cf /dev/null -C "$d" .'
step disk-write dd if=/dev/zero of=/var/tmp/pocket-perf.bin bs=1M count=32 conv=fsync status=none
step disk-read dd if=/var/tmp/pocket-perf.bin of=/dev/null bs=1M status=none
rm /var/tmp/pocket-perf.bin
step memory python3 /usr/local/lib/pocket/guest-memory-check.py
step atomics node /usr/local/lib/pocket/guest-atomics-check.cjs
echo POCKET_ACCEPTANCE_PASS
# Leave a diagnostic window for the host; excluded from per-command timings.
sleep 1
