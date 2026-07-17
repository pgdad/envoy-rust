#!/bin/bash
# Run wrk against the named proxy and concurrently sample `docker stats`
# (CPU% / memory) at ~1Hz for the duration. Prints wrk summary + per-sample
# resource usage + aggregate min/avg/max CPU and memory.
#
# Usage: ./bench-with-stats.sh <envoy|rust>
set -euo pipefail

PROXY="${1:?usage: $0 envoy|rust}"
NET="bench-net"
BACKEND="backend"
PROXY_NAME="proxy-${PROXY}"
DUR="${DUR:-15s}"
CONNS="${CONNS:-100}"
THREADS="${THREADS:-4}"
STATSFILE="/tmp/bench-stats-${PROXY}.out"

cleanup() {
  docker rm -f "$PROXY_NAME" "$BACKEND" 2>/dev/null || true
  docker network rm "$NET" 2>/dev/null || true
}
trap cleanup EXIT
cleanup

docker network create "$NET" >/dev/null

docker run -d --rm --name "$BACKEND" --network "$NET" \
  --cpuset-cpus 0 \
  -v "$(pwd)/backend.conf:/etc/nginx/nginx.conf:ro" \
  nginx:1.27-alpine >/dev/null

sleep 1

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

# Wait for readiness.
for _ in $(seq 1 30); do
  if docker run --rm --network "$NET" curlimages/curl:8.10.1 \
       -fsS "http://$PROXY_NAME:10000/" -o /dev/null 2>/dev/null; then
    break
  fi
  sleep 0.5
done

# Warmup at full concurrency so the connection pools / caches are populated
# when the measured run starts.
docker run --rm --network "$NET" --cpuset-cpus 3,4 \
  williamyeh/wrk -t"$THREADS" -c"$CONNS" -d3s \
  "http://$PROXY_NAME:10000/" >/dev/null 2>&1 || true

# Stats collector: loop `docker stats --no-stream` in the background.
# `--no-stream` does a single CPU snapshot (~1.5s wall-time each call), so
# the loop produces ~1 sample / 1.5s. Avoids TTY escape codes from the
# streaming form and avoids the `wait` deadlock when stopping it.
#
# The deadline is set to the wrk duration. The loop stops checking after
# that; the in-flight `docker stats` call will still complete, but no new
# samples are taken during the cooldown.
dur_secs="${DUR%[!0-9]*}"
: > "$STATSFILE"
(
  end=$(( $(date +%s) + dur_secs ))
  while [ "$(date +%s)" -lt "$end" ]; do
    docker stats --no-stream --no-trunc \
      --format "{{.CPUPerc}}|{{.MemUsage}}" "$PROXY_NAME" \
      </dev/null 2>/dev/null >> "$STATSFILE" || break
  done
) &
STATS_PID=$!

# Run the measured wrk pass.
echo "==> benchmark: $PROXY (threads=$THREADS conns=$CONNS dur=$DUR)"
WRK_OUT=$(docker run --rm --network "$NET" --cpuset-cpus 3,4 \
  williamyeh/wrk -t"$THREADS" -c"$CONNS" -d"$DUR" --latency \
  "http://$PROXY_NAME:10000/")

# Wait for the stats loop to exit naturally (its `end` deadline) or kill it
# if it's lagging. The loop checks its deadline every iteration so it stops
# within ~2s of the deadline.
wait "$STATS_PID" 2>/dev/null || true

echo "$WRK_OUT"
echo
echo "--- per-sample stats ($PROXY) ---"
cat "$STATSFILE"
echo
echo "--- aggregate ($PROXY) ---"
# CPU is a "%" string; memory comes as "123.4MiB / 7.7GiB" — take the first
# field, then convert MiB/GiB → MiB for arithmetic.
awk -F'|' '
BEGIN { n = 0 }
function to_mib(s,   v, u) {
  if (match(s, /^[ ]*([0-9.]+)([A-Za-z]+)/, m)) {
    v = m[1] + 0; u = m[2];
    if (u == "GiB" || u == "GB") return v * 1024;
    if (u == "MiB" || u == "MB") return v;
    if (u == "KiB" || u == "KB") return v / 1024;
    if (u == "B")               return v / 1024 / 1024;
  }
  return 0;
}
/^[ \t]*$/ { next }
NF >= 2 {
  cpu = $1; sub(/%/, "", cpu); cpu += 0
  split($2, parts, " / ")
  mib = to_mib(parts[1])
  # Drop rows where the container is offline (docker stats reports
  # "--%|-- / --" briefly during teardown).
  if (mib <= 0) next
  cpus[n] = cpu
  mems[n] = mib
  n++
}
END {
  if (n == 0) { print "no samples"; exit }
  # Sort numerically (poor-mans bubble — n is small)
  for (i = 0; i < n; i++) for (j = i+1; j < n; j++) {
    if (cpus[j] < cpus[i]) { t = cpus[i]; cpus[i] = cpus[j]; cpus[j] = t }
    if (mems[j] < mems[i]) { t = mems[i]; mems[i] = mems[j]; mems[j] = t }
  }
  cmed = (n % 2 == 1) ? cpus[int(n/2)] : (cpus[n/2-1] + cpus[n/2]) / 2
  mmed = (n % 2 == 1) ? mems[int(n/2)] : (mems[n/2-1] + mems[n/2]) / 2
  printf "samples=%d  CPU%%: min=%.1f  median=%.1f  max=%.1f   mem(MiB): min=%.1f  median=%.1f  max=%.1f\n",
    n, cpus[0], cmed, cpus[n-1], mems[0], mmed, mems[n-1]
}' "$STATSFILE"
