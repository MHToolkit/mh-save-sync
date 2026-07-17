#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

required_files=(
  deploy/compose/compose.yaml
  deploy/compose/compose.tls.yaml
  deploy/compose/Caddyfile
)
for file in "${required_files[@]}"; do
  if [[ ! -s "$file" ]]; then
    echo "missing TLS deployment file: $file" >&2
    exit 1
  fi
done

caddyfile="$(cat deploy/compose/Caddyfile)"
for needle in   '{$MH_SAVE_SYNC_PUBLIC_HOST:localhost}'   'reverse_proxy server:8080'   'Strict-Transport-Security'   'X-Content-Type-Options'   'Referrer-Policy'; do
  if [[ "$caddyfile" != *"$needle"* ]]; then
    echo "Caddyfile missing required directive: $needle" >&2
    exit 2
  fi
done

compose_tls="$(cat deploy/compose/compose.tls.yaml)"
for needle in   'tls-proxy:'   'caddy:2.10.2-alpine'   'condition: service_healthy'   './Caddyfile:/etc/caddy/Caddyfile:ro'   'caddy-data:'   'caddy-config:'   'MH_SAVE_SYNC_TLS_HEALTH_INTERVAL:-30s'   'cpus: "0.25"'   'memory: 128M'; do
  if [[ "$compose_tls" != *"$needle"* ]]; then
    echo "compose.tls.yaml missing required setting: $needle" >&2
    exit 3
  fi
done

compose_config_checked=false
if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
  config_output="$(
    MH_SAVE_SYNC_PUBLIC_HOST=save.example.test     MH_SAVE_SYNC_HTTP_PORT=127.0.0.1:18082     MH_SAVE_SYNC_PUBLIC_HTTP_PORT=8080     MH_SAVE_SYNC_PUBLIC_HTTPS_PORT=8443     docker compose -f deploy/compose/compose.yaml -f deploy/compose/compose.tls.yaml config
  )"
  for needle in     'tls-proxy:'     'save.example.test'     'target: 443'     'published: "8443"'     'target: 8080'     'published: "18082"'     'host_ip: 127.0.0.1'     'caddy-data:'     'caddy-config:'; do
    if [[ "$config_output" != *"$needle"* ]]; then
      echo "compose config missing expected TLS projection: $needle" >&2
      exit 4
    fi
  done
  compose_config_checked=true
fi

printf '{"compose_tls_config_test":true,"static_files_checked":true,"compose_config_checked":%s,"tls_proxy":"caddy","public_ports":[80,443],"upstream":"server:8080"}
' "$compose_config_checked"
