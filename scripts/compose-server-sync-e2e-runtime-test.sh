#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fakebin="$tmp/bin"
mkdir -p "$fakebin"

cat > "$fakebin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "compose" && "${2:-}" == "version" ]]; then
  echo "Docker Compose version v2.fake"
  exit 0
fi
if [[ "${1:-}" == "info" ]]; then
  echo "Cannot connect to the Docker daemon" >&2
  exit 1
fi
if [[ "${1:-}" == "compose" ]]; then
  echo "docker compose must not be selected when docker info fails" >&2
  exit 42
fi
echo "unexpected docker invocation: $*" >&2
exit 43
SH

cat > "$fakebin/podman" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "info" ]]; then
  echo "host: fake-podman"
  exit 0
fi
if [[ "${1:-}" == "compose" && "${2:-}" == "version" ]]; then
  echo "podman-compose fake provider"
  exit 0
fi
if [[ "${1:-}" == "compose" ]]; then
  echo "podman compose $*"
  exit 0
fi
echo "unexpected podman invocation: $*" >&2
exit 44
SH
chmod +x "$fakebin/docker" "$fakebin/podman"

probe() {
  PATH="$fakebin:$PATH" \
    MH_SAVE_SYNC_RUNTIME_PROBE_ONLY=1 \
    "$repo_root/scripts/compose-server-sync-e2e.sh" "$@"
}

auto_output="$(probe)"
python3 - "$auto_output" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
assert data["selected_runtime"] == "podman", data
PY

explicit_podman_output="$(CONTAINER_RUNTIME=podman probe)"
python3 - "$explicit_podman_output" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
assert data["selected_runtime"] == "podman", data
PY

set +e
explicit_docker_output="$(CONTAINER_RUNTIME=docker probe 2>&1)"
explicit_docker_status=$?
set -e
if [[ "$explicit_docker_status" -ne 77 ]]; then
  echo "expected explicit unusable docker to exit 77, got $explicit_docker_status" >&2
  echo "$explicit_docker_output" >&2
  exit 1
fi
if [[ "$explicit_docker_output" != *"BLOCKED: CONTAINER_RUNTIME=docker is not usable"* ]]; then
  echo "expected clear BLOCKED message for explicit unusable docker" >&2
  echo "$explicit_docker_output" >&2
  exit 1
fi

set +e
unsupported_output="$(CONTAINER_RUNTIME=nerdctl probe 2>&1)"
unsupported_status=$?
set -e
if [[ "$unsupported_status" -ne 77 ]]; then
  echo "expected unsupported explicit runtime to exit 77, got $unsupported_status" >&2
  echo "$unsupported_output" >&2
  exit 1
fi
if [[ "$unsupported_output" != *"expected docker or podman"* ]]; then
  echo "expected supported-runtime list in unsupported runtime error" >&2
  echo "$unsupported_output" >&2
  exit 1
fi

echo '{"runtime_probe_test":true,"docker_daemon_failure_falls_back_to_podman":true,"explicit_unusable_runtime_exits_77":true}'
