#!/usr/bin/env bash
# Deterministic, reproducible generator for the 0030-http-filter-jwt-authn
# static test data: an RSA-2048 public JWKS (kid k1) + 4 RS256 JWTs.
#
# TOOLING: openssl (keygen + signing) + python3 stdlib only (base64/json).
# NO PyJWT, NO `cryptography` pip lib. openssl 3.x + python3 are sufficient.
#
# The tokens are STATIC + DETERMINISTIC forever: `exp` is far-future
# (4102444800 = year 2100) or far-past (1500000000 = year 2017), so there is
# ZERO clock sensitivity at test time. Run ONCE; the outputs (jwks.json + the
# 4 .jwt files) are committed. Only the PUBLIC JWKS + tokens are committed —
# the RSA PRIVATE key lives in /tmp and is NOT committed.
#
# Re-running regenerates a fresh RSA key (and thus fresh signatures + a fresh
# JWKS modulus); the committed bytes are one frozen instance. The Docker
# differential is the ultimate proof the set is internally consistent (the
# `valid` probe returns 200 from BOTH proxies).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KEY=/tmp/jwtkey-0030.pem

# 1. RSA-2048 private key (the RSA_PKCS1_2048_8192_SHA256 floor).
openssl genrsa -out "$KEY" 2048 2>/dev/null

# base64url helper (URL-safe, NO padding).
b64url() { python3 -c 'import sys,base64; sys.stdout.write(base64.urlsafe_b64encode(sys.stdin.buffer.read()).decode().rstrip("="))'; }

# 2. JWKS: n = raw big-endian modulus bytes (b64url), e = 65537 = AQAB.
MOD_HEX="$(openssl rsa -in "$KEY" -noout -modulus 2>/dev/null | sed 's/^Modulus=//')"
N_B64URL="$(printf '%s' "$MOD_HEX" | python3 -c 'import sys,base64; h=sys.stdin.read().strip(); sys.stdout.write(base64.urlsafe_b64encode(bytes.fromhex(h)).decode().rstrip("="))')"
printf '{"keys":[{"kty":"RSA","kid":"k1","use":"sig","alg":"RS256","n":"%s","e":"AQAB"}]}' "$N_B64URL" > "$HERE/jwks.json"

# 3. Token builder: sign <b64url-header>.<b64url-payload> with RS256.
HEADER_B64="$(printf '%s' '{"alg":"RS256","kid":"k1","typ":"JWT"}' | b64url)"

mk_jwt() {
  local payload="$1" outfile="$2"
  local pay_b64 signing_input sig_b64
  pay_b64="$(printf '%s' "$payload" | b64url)"
  signing_input="${HEADER_B64}.${pay_b64}"
  sig_b64="$(printf '%s' "$signing_input" | openssl dgst -sha256 -sign "$KEY" -binary | b64url)"
  printf '%s.%s' "$signing_input" "$sig_b64" > "$outfile"
}

mk_jwt '{"iss":"testing@secure.istio.io","aud":["jwt-fixture-aud"],"exp":4102444800}' "$HERE/valid.jwt"
mk_jwt '{"iss":"testing@secure.istio.io","aud":["jwt-fixture-aud"],"exp":1500000000}' "$HERE/expired.jwt"
mk_jwt '{"iss":"testing@secure.istio.io","aud":["other-aud"],"exp":4102444800}'        "$HERE/wrong_aud.jwt"

# 4. tampered.jwt: copy valid.jwt and flip its FIRST signature char to a
#    DIFFERENT valid base64url char (still base64url-decodable, but the
#    signature no longer verifies). The FIRST sig char's value owns the top 6
#    bits of signature byte 0, so a guaranteed-different replacement always
#    alters the decoded signature. NOT the LAST char: a 256-byte RSA signature's
#    final base64url char carries only 2 meaningful bits (the rest are discarded
#    by non-canonical-tolerant base64url decoding), so flipping it can be a
#    no-op for ~1/4 of fresh keys — which would make a regenerated tampered.jwt
#    silently verify.
python3 - "$HERE/valid.jwt" "$HERE/tampered.jwt" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
t = open(src).read()
i = t.rfind('.') + 1                  # first char of the signature segment
repl = 'B' if t[i] == 'A' else 'A'    # guaranteed-different valid base64url char
open(dst, 'w').write(t[:i] + repl + t[i + 1:])
PY

# 5. Sanity: each token has exactly 3 non-empty dot-segments.
for f in valid expired wrong_aud tampered; do
  segs="$(awk -F. '{print NF}' "$HERE/$f.jwt")"
  [ "$segs" -eq 3 ] || { echo "FAIL: $f.jwt has $segs segments" >&2; exit 1; }
done
echo "generated jwks.json + valid/expired/wrong_aud/tampered .jwt in $HERE"
