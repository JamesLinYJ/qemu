#!/bin/bash
# SPDX-License-Identifier: MIT
# Component acceptance image only; this service is not installed in release images.
set -euo pipefail
finish() {
    result=$?
    if [[ $result -ne 0 ]]; then echo "POCKET_ACCEPTANCE_FAIL:$result"; fi
    sync
    systemctl --no-block poweroff
}
trap finish EXIT
step() {
    echo "POCKET_STEP_BEGIN:$1:$(date +%s%N)"
    name=$1; shift
    "$@"
    echo "POCKET_STEP_PASS:$name:$(date +%s%N)"
}
echo "POCKET_KERNEL:$(uname -r)"
[[ $(uname -r) == 7.3.0-pocket ]]
[[ $(cat /proc/1/comm) == systemd ]]
step systemd systemctl --no-pager --failed
step dns getent hosts deb.debian.org
step tls curl --fail --silent --show-error --max-time 45 https://example.com/
step openssl openssl version
step python python3 -c 'import threading,subprocess,ssl; t=threading.Thread(target=lambda: print("thread-ok")); t.start(); t.join(); assert subprocess.check_output(["/bin/sh","-c","printf 42"])==b"42"; print(ssl.OPENSSL_VERSION)'
step memory python3 /usr/local/lib/pocket/guest-memory-check.py
step node node -e 'const {Worker}=require("node:worker_threads"); const w=new Worker("require(\"node:worker_threads\").parentPort.postMessage(42)",{eval:true});w.on("message",m=>{if(m!==42) process.exitCode=1;console.log("node-worker",m)})'
step npm npm --version
step atomics node /usr/local/lib/pocket/guest-atomics-check.cjs
step npm-registry npm view is-number version --fetch-retries=0 --fetch-timeout=45000
step git git ls-remote https://github.com/octocat/Hello-World.git HEAD
step pipeline bash -c 'test "$(printf "a\nb\n" | grep b | tr b B)" = B'
step filesystem bash -c 'd=$(mktemp -d); trap '\''rm -rf "$d"'\'' EXIT; printf A > "$d/Case"; printf B > "$d/case"; ln -s Case "$d/link"; test "$(cat "$d/link")" = A; test "$(cat "$d/case")" = B; chmod 600 "$d/Case"; test "$(stat -c %a "$d/Case")" = 600'
step python-extensions python3 -c 'import numpy,pandas,PIL,docx,openpyxl,reportlab,pypdf; print(numpy.__version__,pandas.__version__,PIL.__version__)'
step apt-update apt-get update
step apt-install env DEBIAN_FRONTEND=noninteractive apt-get install -y --reinstall --no-install-recommends hello
step installed-package hello
echo POCKET_ACCEPTANCE_PASS
