#!/bin/bash
set -euo pipefail
# Loom container firewall — IPv4 default-deny outbound with a domain
# allowlist resolved to an ipset, plus a sidecar that periodically
# re-resolves the allowlist and atomically swaps the ipset.
#
# IPV4-ONLY (intentional). The allowlist is resolved against A records;
# AAAA-only domains are unreachable by design. ip6tables policy is
# default-deny on every chain so a misconfigured IPv6 route cannot bypass
# the IPv4 firewall.
#
# Run as root via the entrypoint. After this script exits, the entrypoint
# removes /etc/sudoers.d/loom-firewall so the unprivileged loom user has
# no path to re-invoke us (closes M8).

# ----- single-instance lock -------------------------------------------------
# flock prevents a second `loom-firewall.sh` from racing with the first
# (M7 — the original lockfile was a touch-test that lost the race). Path
# is 0400 owned by root once acquired.
LOCK=/var/run/loom-firewall.lock
exec 9>"$LOCK"
chmod 0400 "$LOCK" 2>/dev/null || true
if ! flock -n 9; then
  echo "loom-firewall.sh: another instance is running; exiting." >&2
  exit 0
fi

POLICY=/etc/loom/network/allowed_domains.txt
ALWAYS=(api.anthropic.com registry.npmjs.org)

# Bound the allowlist file size before we read it — refuses a multi-MB
# policy file that could exhaust ipset capacity or stall DNS resolution.
MAX_ALLOWLIST_BYTES=65536
if [ -f "$POLICY" ]; then
  policy_size=$(stat -c '%s' "$POLICY" 2>/dev/null || stat -f '%z' "$POLICY" 2>/dev/null || echo 0)
  if [ "${policy_size:-0}" -gt "$MAX_ALLOWLIST_BYTES" ]; then
    echo "loom-firewall.sh: allowlist file ${POLICY} is ${policy_size} bytes" >&2
    echo "  (limit ${MAX_ALLOWLIST_BYTES}). Refusing to load." >&2
    exit 1
  fi
fi

# Maximum total IPs the resolved ipset may hold. Prevents DNS flood from
# blowing up the ipset and exhausting kernel memory.
MAX_TOTAL_IPS=4096

# ----- ip6tables: default deny everywhere ----------------------------------
# Per finding #17: IPv6 allowlist intentionally not maintained. Drop every
# chain so AAAA traffic never bypasses the IPv4 allowlist.
ip6tables -P OUTPUT DROP
ip6tables -P FORWARD DROP
ip6tables -P INPUT DROP

# ----- iptables base policy ------------------------------------------------
iptables -P OUTPUT DROP
iptables -P FORWARD DROP
iptables -A OUTPUT -o lo -j ACCEPT

# DNS to the configured nameservers only — public DoH resolvers stay
# blocked unless explicitly allowlisted. CRITICAL: this block MUST precede
# the RFC1918 REJECTs below (rootless Podman's DNS proxy lives in 10.0.0.0/8).
for ns in $(awk '/^nameserver/ {print $2}' /etc/resolv.conf); do
  iptables -A OUTPUT -p udp -d "$ns" --dport 53 -j ACCEPT
  iptables -A OUTPUT -p tcp -d "$ns" --dport 53 -j ACCEPT
done

# Cloud metadata IP — never reachable from a sandboxed agent.
iptables -A OUTPUT -d 169.254.169.254 -j REJECT --reject-with icmp-net-unreachable

# RFC1918 (private networks) except loopback / DNS exemptions above.
iptables -A OUTPUT -d 10.0.0.0/8 -j REJECT --reject-with icmp-net-unreachable
iptables -A OUTPUT -d 172.16.0.0/12 -j REJECT --reject-with icmp-net-unreachable
iptables -A OUTPUT -d 192.168.0.0/16 -j REJECT --reject-with icmp-net-unreachable

# ----- initial ipset build -------------------------------------------------
# `dig +timeout=3 +tries=1` is critical: an unbounded dig + a
# misconfigured DNS path locks this loop forever and the container never
# reaches gosu. We've shipped this in the wrong order once already.
ipset create loom-allowed hash:ip family inet -exist
ipset flush loom-allowed
total=0
for domain in "${ALWAYS[@]}" $(grep -v '^#' "$POLICY" 2>/dev/null || true); do
  # Skip obviously bogus entries (shell metacharacters, spaces).
  if [[ ! "$domain" =~ ^[A-Za-z0-9._-]+$ ]]; then
    continue
  fi
  for ip in $(dig +short +timeout=3 +tries=1 "$domain" A 2>/dev/null | grep -E '^[0-9.]+$'); do
    if [ "$total" -ge "$MAX_TOTAL_IPS" ]; then
      echo "loom-firewall.sh: hit MAX_TOTAL_IPS=${MAX_TOTAL_IPS}; truncating." >&2
      break 2
    fi
    if ipset add loom-allowed "$ip" 2>/dev/null; then
      total=$((total + 1))
    fi
  done
done

# Accept allowlisted egress, reject everything else with a clear ICMP.
iptables -A OUTPUT -m set --match-set loom-allowed dst -j ACCEPT
iptables -A OUTPUT -j REJECT --reject-with icmp-net-unreachable

# ----- background DNS re-resolver sidecar ----------------------------------
# Periodically rebuild the allowlist into a TEMP ipset (loom-allowed-next),
# then atomically swap it in. On DNS failure we leave the old set intact
# rather than flushing — half a working allowlist is better than no
# allowlist at all. The sidecar PID is recorded at /var/run/loom-firewall-resolver.pid
# with a flock so a double-start is impossible.
#
# `setsid nohup ... &` is required because entrypoint.sh `exec gosu loom`
# replaces the entrypoint's process image. Without setsid the sidecar
# inherits the entrypoint's controlling TTY and exits with SIGHUP when
# gosu execs.
RESOLVER_PID_FILE=/var/run/loom-firewall-resolver.pid
RESOLVER_LOCK=/var/run/loom-firewall-resolver.lock
REFRESH_INTERVAL_SECONDS=${LOOM_DNS_REFRESH_INTERVAL:-300}

# Inline resolver loop. Quoted heredoc means $vars expand at sidecar runtime,
# not at script-install time.
resolver_loop() {
  exec 8>"$RESOLVER_LOCK"
  if ! flock -n 8; then
    exit 0
  fi
  echo $$ > "$RESOLVER_PID_FILE"
  chmod 0400 "$RESOLVER_PID_FILE" 2>/dev/null || true
  while sleep "$REFRESH_INTERVAL_SECONDS"; do
    if [ -f "$POLICY" ]; then
      sz=$(stat -c '%s' "$POLICY" 2>/dev/null || echo 0)
      if [ "${sz:-0}" -gt "$MAX_ALLOWLIST_BYTES" ]; then
        continue
      fi
    fi
    ipset create loom-allowed-next hash:ip family inet -exist
    ipset flush loom-allowed-next
    new_total=0
    refresh_ok=1
    for dom in "${ALWAYS[@]}" $(grep -v '^#' "$POLICY" 2>/dev/null || true); do
      if [[ ! "$dom" =~ ^[A-Za-z0-9._-]+$ ]]; then
        continue
      fi
      ips=$(dig +short +timeout=3 +tries=1 "$dom" A 2>/dev/null | grep -E '^[0-9.]+$' || true)
      if [ -z "$ips" ]; then
        # Treat empty A record set as a transient DNS failure — keep old.
        continue
      fi
      for ip in $ips; do
        if [ "$new_total" -ge "$MAX_TOTAL_IPS" ]; then
          break 2
        fi
        if ipset add loom-allowed-next "$ip" 2>/dev/null; then
          new_total=$((new_total + 1))
        fi
      done
    done
    if [ "$refresh_ok" -eq 1 ] && [ "$new_total" -gt 0 ]; then
      ipset swap loom-allowed loom-allowed-next 2>/dev/null || true
      ipset destroy loom-allowed-next 2>/dev/null || true
    else
      ipset destroy loom-allowed-next 2>/dev/null || true
    fi
  done
}

# Export the function and required env so the inline subshell sees them.
export -f resolver_loop
export POLICY MAX_ALLOWLIST_BYTES MAX_TOTAL_IPS REFRESH_INTERVAL_SECONDS RESOLVER_PID_FILE RESOLVER_LOCK
export ALWAYS_LIST="${ALWAYS[*]}"

# setsid + nohup + & detaches from entrypoint's session so the sidecar
# survives `exec gosu loom`.
setsid nohup bash -c 'ALWAYS=($ALWAYS_LIST); resolver_loop' </dev/null >/dev/null 2>&1 &
disown || true
