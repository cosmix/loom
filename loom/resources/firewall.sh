#!/bin/bash
set -euo pipefail

LOCK=/var/run/loom-firewall.lock
# Idempotency lockfile so the unprivileged loom user cannot rerun this
# script later (per finding #16 — once the firewall is installed it must
# stay installed for the container lifetime).
if [ -e "$LOCK" ]; then
  exit 0
fi
touch "$LOCK"
chmod 0400 "$LOCK"

POLICY=/etc/loom/network/allowed_domains.txt
ALWAYS=(api.anthropic.com registry.npmjs.org)

# Per finding #17: deny IPv6 entirely. The allowlist is IPv4-only;
# leaving IPv6 with a default-allow would silently bypass the firewall.
ip6tables -P OUTPUT DROP
ip6tables -P FORWARD DROP
ip6tables -P INPUT DROP

# IPv4: default deny outbound + forward, allow loopback.
iptables -P OUTPUT DROP
iptables -P FORWARD DROP
iptables -A OUTPUT -o lo -j ACCEPT

# Per finding #17: block cloud metadata IP. AWS / GCP / Azure all expose
# instance metadata at 169.254.169.254 — must never reach it from inside
# a sandboxed agent container.
iptables -A OUTPUT -d 169.254.169.254 -j REJECT --reject-with icmp-net-unreachable

# Per finding #17: block RFC1918 (private networks) except loopback.
# Prevents an agent from probing the host's internal network.
iptables -A OUTPUT -d 10.0.0.0/8 -j REJECT --reject-with icmp-net-unreachable
iptables -A OUTPUT -d 172.16.0.0/12 -j REJECT --reject-with icmp-net-unreachable
iptables -A OUTPUT -d 192.168.0.0/16 -j REJECT --reject-with icmp-net-unreachable

# Per finding #17: DNS only to the nameservers in /etc/resolv.conf.
# Do NOT permit traffic to public DoH resolvers (Cloudflare 1.1.1.1,
# Google 8.8.8.8, etc.) unless the agent allowlists them — otherwise the
# agent could exfiltrate via DNS-over-HTTPS.
for ns in $(awk '/^nameserver/ {print $2}' /etc/resolv.conf); do
  iptables -A OUTPUT -p udp -d "$ns" --dport 53 -j ACCEPT
  iptables -A OUTPUT -p tcp -d "$ns" --dport 53 -j ACCEPT
done

# Build allowlist ipset.
ipset create loom-allowed hash:ip family inet -exist
ipset flush loom-allowed
for domain in "${ALWAYS[@]}" $(grep -v '^#' "$POLICY" 2>/dev/null || true); do
  for ip in $(dig +short "$domain" A | grep -E '^[0-9.]+$'); do
    ipset add loom-allowed "$ip" 2>/dev/null || true
  done
done

# Accept traffic to allowlisted IPs.
iptables -A OUTPUT -m set --match-set loom-allowed dst -j ACCEPT

# Final reject for everything else (clearer error than DROP).
iptables -A OUTPUT -j REJECT --reject-with icmp-net-unreachable
