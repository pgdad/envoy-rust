#!/bin/bash
# Usage: ./run-bench.sh <envoy|rust>
set -euo pipefail

PROXY="${1:?usage: $0 envoy|rust}"
NET="bench-net"
BACKEND="backend"
PROXY_NAME="proxy-${PROXY}"
DUR="${DUR:-15s}"
CONNS="${CONNS:-100}"
THREADS="${THREADS:-4}"

cleanup() {
  docker rm -f "$PROXY_NAME" "$BACKEND" 2>/dev/null || true
  docker network rm "$NET" 2>/dev/null || true
}
trap cleanup EXIT
cleanup

docker network create "$NET" >/dev/null

# Backend (nginx). Pin to single CPU for determinism across runs.
docker run -d --rm --name "$BACKEND" --network "$NET" \
  --cpuset-cpus 0 \
  -v "$(pwd)/backend.conf:/etc/nginx/nginx.conf:ro" \
  nginx:1.27-alpine >/dev/null

sleep 1

# Proxy
case "$PROXY" in
  envoy)
    docker run -d --rm --name "$PROXY_NAME" --network "$NET" \
      --cpuset-cpus 1,2 \
      -v "$(pwd)/envoy.yaml:/etc/envoy/envoy.yaml:ro" \
      envoyproxy/envoy:v1.33.0 \
      -c /etc/envoy/envoy.yaml --concurrency 2 >/dev/null
    ;;
  rust)
    docker run -d --rm --name "$PROXY_NAME" --network "$NET" \
      --cpuset-cpus 1,2 \
      --sysctl net.ipv4.tcp_tw_reuse=1 \
      --sysctl net.ipv4.ip_local_port_range="1024 65535" \
      -e TOKIO_WORKER_THREADS=2 \
      envoy-rust:bench >/dev/null
    ;;
  *) echo "unknown proxy: $PROXY"; exit 2 ;;
esac

# Wait for proxy to be ready
for i in $(seq 1 30); do
  if docker run --rm --network "$NET" curlimages/curl:8.10.1 -fsS "http://$PROXY_NAME:10000/" -o /dev/null 2>/dev/null; then
    break
  fi
  sleep 0.5
done

echo "==> warmup"
docker run --rm --network "$NET" --cpuset-cpus 3,4 \
  williamyeh/wrk -t"$THREADS" -c"$CONNS" -d3s "http://$PROXY_NAME:10000/" >/dev/null 2>&1 || true

echo "==> benchmark: $PROXY (threads=$THREADS conns=$CONNS dur=$DUR)"
docker run --rm --network "$NET" --cpuset-cpus 3,4 \
  williamyeh/wrk -t"$THREADS" -c"$CONNS" -d"$DUR" --latency "http://$PROXY_NAME:10000/"
