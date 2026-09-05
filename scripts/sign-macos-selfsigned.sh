#!/usr/bin/env bash
#
# Create a self-signed code-signing certificate on a macOS CI runner and
# install it into the keychain so `codesign --sign "..."` produces a
# Developer-Identity-style signature (a real X.509 cert with a private key),
# NOT an ad-hoc one. This is what lets a non-Developer-ID app show up as
# "unidentified developer" (Open Anyway is available) instead of "damaged".
#
# macOS's `security import` is strict about the PKCS12 it will accept:
#   * LibreSSL 3.x / OpenSSL 3 / `cryptography` all emit a *non-standard*
#     macData (a modern PBKDF2/AES MAC, plus an extra iteration INTEGER),
#     which `security import` rejects ("MAC verification failed").
#   * A macData-less PKCS12 is rejected too ("Unknown format in import").
#
# So we build the standard PKCS12 structure ourselves: keep `cryptography`'s
# standard `[0]`-EXPLICIT authSafe (key + cert bags), and attach a *standard*
# macData whose MAC is computed with the **legacy PKCS#12 KDF** (SHA-1, the
# one macOS actually verifies). Because the exact KDF parameters macOS expects
# (iteration count, whether it reads the macSalt, BMP vs UTF-8 password) vary
# by implementation, we emit several candidate PKCS12 files and import the
# first one `security import` accepts.
#
# Run from the repo root on a macOS runner:
#   bash scripts/sign-macos-selfsigned.sh
#
# Requires: openssl (genrsa/req), python3 with `cryptography`.
set -euo pipefail

CERT_CN="Banana Hand Self-Signed Dev"

WORK="$(mktemp -d /tmp/banana-hand-sign.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
KEY="$WORK/key.pem"
CERT="$WORK/cert.pem"
# Random, never-persisted password: protects the key bag inside the PKCS12.
PASS="$(openssl rand -hex 8)"

echo "==> [1/4] Generating 4096-bit RSA key"
openssl genrsa -out "$KEY" 4096 >/dev/null 2>&1

echo "==> [2/4] Self-signing the code-signing certificate (10-year validity)"
openssl req -new -x509 -key "$KEY" -out "$CERT" -days 3650 -sha256 \
  -subj "/CN=${CERT_CN}" \
  -addext "extendedKeyUsage=codeSigning" \
  -addext "keyUsage=digitalSignature" \
  -addext "basicConstraints=critical,CA:FALSE" >/dev/null 2>&1

echo "==> [3/4] Building standard-macData PKCS12 (legacy SHA-1 KDF)"
python3 - "$PASS" "$KEY" "$CERT" "$WORK" <<'PY'
import hashlib, hmac, os, sys

password, key_pem_path, cert_pem_path, out_dir = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]

# ---------- PKCS#12 KDF (RFC 7292 / PKCS#12 B.1.3) ----------
def _rep(v, n):
    if n == 0: return b""
    if not v: return b"\x00" * n
    return (v * ((n + len(v) - 1) // len(v)))[:n]

def kdf(password, salt, id_byte, iterations, length=20, hash_name="sha1"):
    H = getattr(hashlib, hash_name); u = H().digest_size; v = H().block_size
    D = bytes([id_byte]) * v
    S = _rep(salt, v * ((len(salt) + v - 1) // v)) if salt else b""
    P = _rep(password, v * ((len(password) + v - 1) // v)) if password else b""
    I = bytearray(S + P); out = bytearray()
    for _ in range((length + u - 1) // u):
        A = H(D + I).digest()
        for _ in range(iterations - 1):
            A = H(A).digest()
        out += A
        B = _rep(A, v)
        for off in range(0, len(I), v):
            blk = bytearray(I[off:off + v]); carry = 1
            for j in range(v - 1, -1, -1):
                val = blk[j] + B[j] + carry
                blk[j] = val & 0xFF; carry = val >> 8
            I[off:off + v] = blk
    return bytes(out[:length])

def mac(pw, salt, iterations, message):
    return hmac.new(kdf(pw, salt, 3, iterations, 20), message, hashlib.sha1).digest()

def pw_bmp(pw):  return pw.encode("utf-16-be") + b"\x00\x00"
def pw_utf8(pw): return pw.encode("utf-8")

# ---------- minimal DER ----------
def der_len(n):
    if n < 0x80: return bytes([n])
    b = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return bytes([0x80 | len(b)]) + b

def der(tag, content): return bytes([tag]) + der_len(len(content)) + content

def parse(d, off):
    tag = d[off]; off += 1
    ln = d[off]; off += 1
    if ln & 0x80:
        n = ln & 0x7f; ln = int.from_bytes(d[off:off + n], "big"); off += n
    return tag, d[off:off + ln], off + ln

def field(d, off):
    """Return (new_off, raw_field); raw_field includes tag+length+content."""
    start = off; off += 1
    ln = d[off]; off += 1
    if ln & 0x80:
        n = ln & 0x7f; ln = int.from_bytes(d[off:off + n], "big"); off += n
    off += ln
    return off, d[start:off]

SHA1_OID = bytes.fromhex("2b0e03021a")  # 1.3.14.3.2.26

def build_macdata(digest, mac_salt):
    alg = der(0x30, der(0x06, SHA1_OID) + der(0x05, b""))  # DigestAlgorithmIdentifier
    body = alg + der(0x04, digest)
    if mac_salt is not None:
        body += der(0xA0, mac_salt)  # [0] IMPLICIT macSalt
    return der(0x30, body)

# ---------- build the base PKCS12 with `cryptography` (standard authSafe) ----------
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.serialization import pkcs12, BestAvailableEncryption
from cryptography.x509 import load_pem_x509_certificate

key = serialization.load_pem_private_key(open(key_pem_path, "rb").read(), password=None)
cert = load_pem_x509_certificate(open(cert_pem_path, "rb").read())
p12 = pkcs12.serialize_key_and_certificates(
    "Banana Hand".encode("utf-8"), key, cert, None, BestAvailableEncryption(password.encode("utf-8"))
)

# Keep the version and authSafe fields byte-identical to cryptography's valid
# output; only the macData is replaced (re-wrapping the authSafe corrupts it).
_, body, _ = parse(p12, 0)
o = 0
o, ver_field = field(body, o)
o, authSafe_field = field(body, o)
# authSafe content (for the "content" MAC variant); works for [0] or plain alike.
_, authSafe_content, _ = parse(authSafe_field, 0)

# ---------- emit candidate PKCS12 files ----------
# (iterations, use_salt, mac_over_field, password_encoding)
variants = [
    (2048,   True,  True,  "bmp"),
    (2048,   True,  False, "bmp"),
    (2048,   False, True,  "bmp"),
    (3,      True,  True,  "bmp"),
    (50000,  True,  True,  "bmp"),
    (2048,   True,  True,  "utf8"),
]
for i, (iterations, use_salt, over_field, pwenc) in enumerate(variants):
    msg = authSafe_field if over_field else authSafe_content
    mac_salt = os.urandom(16) if use_salt else b""
    pw = pw_bmp(password) if pwenc == "bmp" else pw_utf8(password)
    mac_bytes = mac(pw, mac_salt, iterations, msg)
    macData = der(0xA1, build_macdata(mac_bytes, mac_salt if use_salt else None))
    out = der(0x30, ver_field + authSafe_field + macData)
    path = os.path.join(out_dir, f"v{i}.p12")
    with open(path, "wb") as f:
        f.write(out)
    print(f"  v{i}: iterations={iterations:<6} salt={'yes' if use_salt else 'no ':<3} "
          f"msg={'field' if over_field else 'content'} pw={pwenc} len={len(out)}")
print(f"==> wrote {len(variants)} PKCS12 variants")
# Diagnostics: is cryptography's RAW output valid, and where does v0 diverge?
import subprocess
def _probe(label, data, fname):
    p = os.path.join(out_dir, fname)
    with open(p, "wb") as fh:
        fh.write(data)
    try:
        r = subprocess.run(
            ["openssl", "pkcs12", "-info", "-in", p, "-passin", f"pass:{password}"],
            capture_output=True, text=True, timeout=30)
        print(f"  [openssl -info {label}] rc={r.returncode}")
        for line in (r.stdout + r.stderr).splitlines()[:8]:
            print("   |", line)
    except Exception as e:
        print(f"  [openssl -info {label}] error: {e}")
    print(f"  [{label} hex0:48] {data[:48].hex()}")

_probe("raw-cryptography", p12, "raw.p12")
_probe("v0", open(os.path.join(out_dir, "v0.p12"), "rb").read(), "v0.p12")
PY

echo "==> [4/4] Installing into keychain (trying each variant)"
# Import the first variant `security import` accepts. A macData whose MAC macOS
# cannot verify is rejected entirely (nothing is added), so we can safely try
# each candidate against the same keychain until a real identity appears.
# `security import` here takes the input file first, then options; -P reads the
# password from a file, so write the (random) password to a temp file once.
printf '%s' "$PASS" > "$WORK/passfile"
security unlock-keychain -p "" 2>/dev/null || true
found=""
# Try the RAW cryptography file first (decisive: shows macOS's verdict on the
# authSafe structure), then the standard-macData variants.
names=("raw" "v0" "v1" "v2" "v3" "v4" "v5")
for i in "${!names[@]}"; do
  n="${names[$i]}"
  f="$WORK/$n.p12"
  echo "  trying $n ..."
  if ! security import "$f" -T /usr/bin/codesign -P "$WORK/passfile" -A 2>"$WORK/import-err"; then
    echo "    $n import failed: $(head -1 "$WORK/import-err" 2>/dev/null)"
  fi
  if security find-identity -p codesigning 2>/dev/null | grep -q "$CERT_CN"; then
    echo "  ==> identity imported from $n"
    found="$n"
    break
  fi
done

if [ -n "$found" ]; then
  echo "==> Self-signed code-signing identity installed:"
  security find-identity -p codesigning
else
  echo "ERROR: no PKCS12 variant was accepted by security import." >&2
  echo "--- last import stderr ---" >&2
  cat "$WORK/import-err" 2>/dev/null >&2 || true
  echo "--- current identities ---" >&2
  security find-identity 2>/dev/null >&2 || true
  echo "--- v0 head (hex) ---" >&2
  xxd "$WORK/v0.p12" 2>/dev/null | head -20 >&2 || true
  exit 1
fi

echo "==> Done. codesign --sign \"${CERT_CN}\" is available."
