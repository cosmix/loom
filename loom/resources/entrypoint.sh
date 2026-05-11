#!/bin/bash
set -euo pipefail
# Per finding #15: run as root only long enough to install the firewall,
# then drop privileges via gosu. The unprivileged "loom" user cannot
# rerun loom-firewall.sh because of the root-owned lockfile (#16).
/usr/local/bin/loom-firewall.sh
exec gosu loom "$@"
