#!/usr/bin/env bash
# Generate a self-signed Apple code-signing certificate, import it into the
# login keychain, and print the codesign identity name to stdout.
#
# WHY a self-signed cert instead of ad-hoc ("-"):
# macOS 15+/26 (Tahoe) classifies a *quarantined, ad-hoc-signed* app as
# "damaged and can't be opened" — that dialog has no Open Anyway path.
# A *real* X.509 certificate (even one from an untrusted/self CA) is instead
# classified as "from an unidentified developer," which DOES expose the
# System Settings > Privacy & Security > "Open Anyway" button. That lets a
# non-notarized app be approved without a paid Apple Developer ID.
#
# The key is generated fresh on each CI runner (no secret stored), so the
# cdhash changes per build; users may need to re-approve after an update.
#
# PKCS12 import note: OpenSSL 3 / LibreSSL 3.x append a NON-STANDARD
# iteration-count INTEGER to the macData and use a PBKDF2-style MAC KDF that
# Apple's `security import` (legacy PKCS12 KDF) rejects with "MAC verification
# failed." The macData is optional per PKCS#12, so we build the PKCS12 with
# Python `cryptography`, STRIP the macData, and import the MAC-less file.
set -euo pipefail

CERT_NAME="Banana Hand Self-Signed Dev"
WORKDIR="$(mktemp -d /tmp/bh-selfsigned.XXXXXX)"
trap 'rm -rf "${WORKDIR}"' EXIT
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"

# 1. 2048-bit RSA key.
openssl genrsa -out "${WORKDIR}/key.pem" 2048

# 2. Self-signed cert carrying the code-signing extended key usage.
openssl req -x509 -new -key "${WORKDIR}/key.pem" -sha256 -days 3650 \
  -out "${WORKDIR}/cert.pem" \
  -subj "/CN=${CERT_NAME}/O=Banana Hand/OU=Dev" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=codeSigning" \
  -addext "subjectKeyIdentifier=hash"

# 3. Ensure Python `cryptography` is available.
if ! python3 -c "import cryptography" >/dev/null 2>&1; then
  pip3 install --quiet cryptography 2>/dev/null \
    || python3 -m pip install --quiet cryptography 2>/dev/null \
    || pip3 install --quiet --break-system-packages cryptography 2>/dev/null \
    || python3 -m pip install --quiet --break-system-packages cryptography
fi
python3 -c "import cryptography" >/dev/null 2>&1 \
  || { echo "ERROR: could not import Python 'cryptography' on this runner." >&2; exit 1; }

# 4. Build the PKCS12 (unencrypted key) and strip the macData so macOS
#    `security import` can load it without a KDF/MAC check.
python3 - "${WORKDIR}" <<'PY'
import sys
w = sys.argv[1]
from cryptography.hazmat.primitives.serialization import (
    load_pem_private_key, NoEncryption, pkcs12,
)
try:
    from cryptography.hazmat.primitives.serialization import (
        load_pem_x509_certificate,
    )
except ImportError:  # older cryptography: loader lives in the x509 module
    from cryptography import x509
    load_pem_x509_certificate = x509.load_pem_x509_certificate

key = load_pem_private_key(open(f"{w}/key.pem", "rb").read(), None)
cert = load_pem_x509_certificate(open(f"{w}/cert.pem", "rb").read())
p12 = pkcs12.serialize_key_and_certificates(
    b"Banana Hand Self-Signed Dev", key, cert, None, NoEncryption()
)

# --- strip the macData (optional per PKCS#12; macOS rejects modern MACs) ---
DIGEST_OIDS = {
    bytes.fromhex("608648016503040201"),  # SHA-256
    bytes.fromhex("608648016503040200"),  # SHA-1 (NIST)
    bytes.fromhex("2b0e03021a"),          # SHA-1
    bytes.fromhex("2a864886f70d010505"),  # SHA-1 (alt)
    bytes.fromhex("2a864886f70d010104"),  # MD5
    bytes.fromhex("2b0e03021d"),          # MD5
}
def _tlv(d, off):
    tag = d[off]; off += 1
    ln = d[off]; off += 1
    if ln & 0x80:
        n = ln & 0x7F
        ln = int.from_bytes(d[off:off + n], "big"); off += n
    return tag, d[off:off + ln], off + ln
def _mk(tag, c):
    ln = len(c)
    if ln < 0x80:
        return bytes([tag, ln]) + c
    n = (ln.bit_length() + 7) // 8
    return bytes([tag, 0x80 | n]) + ln.to_bytes(n, "big") + c
def _is_digest_alg(seq):
    t, c, _ = _tlv(seq, 0)
    if t != 0x30:
        return False
    ot, oc, _ = _tlv(c, 0)
    return ot == 0x06 and oc in DIGEST_OIDS
def _is_macdata(seq):
    t, c, off = _tlv(seq, 0)
    if t != 0x30 or not _is_digest_alg(c):
        return False
    t2, _, off = _tlv(seq, off)
    if t2 != 0x04:
        return False
    if off < len(seq):
        t3, _, _ = _tlv(seq, off)
        if t3 not in (0x04, 0x02):
            return False
    return True

_, body, _ = _tlv(p12, 0)
off = 0
kids = []
while off < len(body):
    t, c, off = _tlv(body, off)
    if t == 0x30 and _is_macdata(c):
        continue  # drop the macData
    kids.append((t, c))
p12 = _mk(0x30, b"".join(_mk(t, c) for t, c in kids))
open(f"{w}/cert.p12", "wb").write(p12)
print("MAC-less PKCS12 written:", len(p12), "bytes")
PY

# 5. Import into the login keychain and grant codesign access to the key.
security unlock-keychain -p "" "${KEYCHAIN}" >/dev/null 2>&1 || true
if ! security import "${WORKDIR}/cert.p12" -k "${KEYCHAIN}" \
    -T /usr/bin/codesign -T /usr/bin/productsign -P "" -A; then
  echo "ERROR: security import failed. Current identities:" >&2
  security find-identity -v -p codesigning >&2 || true
  echo "PKCS12 head (hex):" >&2
  xxd "${WORKDIR}/cert.p12" 2>/dev/null | head -4 >&2 || true
  exit 1
fi
security set-key-partition-list -S "apple-tool:,apple:" -k "" -A "${KEYCHAIN}" >/dev/null 2>&1 || true

echo "--- available codesign identities ---"
security find-identity -v -p codesigning || true
echo "--- selected identity ---"
if ! security find-identity -v -p codesigning 2>/dev/null | grep -q "${CERT_NAME}"; then
  echo "ERROR: '${CERT_NAME}' is not in the codesigning identities after import." >&2
  echo "The self-signed certificate could not be imported; the build cannot sign." >&2
  exit 1
fi
echo "${CERT_NAME}"
