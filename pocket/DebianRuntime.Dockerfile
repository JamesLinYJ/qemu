# SPDX-License-Identifier: MIT
FROM debian:trixie-slim@sha256:a66b19aa552f595b4ab276c6b4f9b24fa4546632c5ecd88cccdced4a6866323a
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && chmod 755 /usr/sbin/policy-rc.d \
    && apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    bash bash-completion coreutils findutils diffutils sed grep gawk util-linux \
    systemd systemd-sysv systemd-resolved dbus udev \
    iproute2 iputils-ping procps psmisc openssh-client openssh-server \
    git curl wget openssl ca-certificates ripgrep jq sqlite3 \
    python3 python3-pip python3-venv python3-dev python3-numpy python3-pandas \
    python3-pil python3-openpyxl python3-docx python3-reportlab python3-pypdf \
    build-essential cmake ninja-build pkg-config libssl-dev libffi-dev \
    tar gzip bzip2 xz-utils unzip zip file less nano locales \
    && rm -f /usr/sbin/policy-rc.d \
    && apt-get clean && rm -rf /var/lib/apt/lists/* \
    && dpkg-query -W > /pocket-packages.tsv
RUN test -x /usr/lib/systemd/systemd-networkd
COPY node-v24.19.0-linux-arm64.tar.xz /tmp/node.tar.xz
RUN echo '01443c1e1a29e531ccad5a46fefa6df490d2189c49f7955904aecdbb0fe86fdc  /tmp/node.tar.xz' | sha256sum -c - \
    && mkdir -p /opt/pocketagent/node24 \
    && tar -xJf /tmp/node.tar.xz --strip-components=1 -C /opt/pocketagent/node24 \
    && chown -R 0:0 /opt/pocketagent/node24 \
    && rm /tmp/node.tar.xz \
    && ln -s /opt/pocketagent/node24/bin/node /usr/local/bin/node \
    && ln -s /opt/pocketagent/node24/bin/npm /usr/local/bin/npm \
    && ln -s /opt/pocketagent/node24/bin/npx /usr/local/bin/npx
RUN mkdir -p /etc/systemd/network /root/Workspaces \
    && printf '[Match]\nName=eth* en*\n[Link]\nRequiredFamilyForOnline=ipv4\n[Network]\nDHCP=yes\nIPv6AcceptRA=yes\n' > /etc/systemd/network/20-virtio.network \
    && systemctl --root=/ enable systemd-networkd.service systemd-resolved.service \
    && systemctl --root=/ disable ssh.service ssh.socket \
    && systemctl --root=/ set-default multi-user.target \
    && printf 'pocketagent\n' > /etc/hostname \
    && printf '127.0.0.1 localhost\n127.0.1.1 pocketagent\n::1 localhost ip6-localhost\n' > /etc/hosts \
    && truncate -s 0 /etc/machine-id \
    && rm -f /var/lib/dbus/machine-id /etc/ssh/ssh_host_* /var/lib/systemd/random-seed
ENV LANG=C.UTF-8
