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
# PKCS12 is built with Python `cryptography` (Rust/OpenSSL backend), NOT macOS
# LibreSSL `openssl`: LibreSSL 3.x writes a PKCS12 whose MAC KDF and structure
# are rejected by Apple's `security import` ("MAC verification failed" /
# "unknown format"). `cryptography` uses the spec PKCS12 KDF that Apple
# verifies. A random non-empty password sidesteps empty-password KDF edges.
set -euo pipefail

CERT_NAME="Banana Hand Self-Signed Dev"
WORKDIR="$(mktemp -d /tmp/bh-selfsigned.XXXXXX)"
trap 'rm -rf "${WORKDIR}"' EXIT
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
P12PASS="$(openssl rand -hex 16)"

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

# 3. Ensure Python `cryptography` is available (Apple-compatible PKCS12).
if ! python3 -c "import cryptography" >/dev/null 2>&1; then
  pip3 install --quiet cryptography 2>/dev/null \
    || python3 -m pip install --quiet cryptography 2>/dev/null \
    || pip3 install --quiet --break-system-packages cryptography 2>/dev/null \
    || python3 -m pip install --quiet --break-system-packages cryptography
fi
python3 -c "import cryptography" >/dev/null 2>&1 \
  || { echo "ERROR: could not import Python 'cryptography' on this runner." >&2; exit 1; }

# 4. Serialize key+cert into an Apple-compatible, password-protected PKCS12.
P12PASS="${P12PASS}" python3 - "${WORKDIR}" <<'PY'
import os, sys
w = sys.argv[1]
from cryptography.hazmat.primitives.serialization import (
    load_pem_private_key,
    BestAvailableEncryption,
    pkcs12,
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
    b"Banana Hand Self-Signed Dev",
    key,
    cert,
    None,
    BestAvailableEncryption(os.environ["P12PASS"].encode()),
)
open(f"{w}/cert.p12", "wb").write(p12)
print("PKCS12 written:", len(p12), "bytes")
PY

# 5. Import into the login keychain and grant codesign access to the key.
security unlock-keychain -p "" "${KEYCHAIN}" >/dev/null 2>&1 || true
if ! security import "${WORKDIR}/cert.p12" -k "${KEYCHAIN}" \
    -T /usr/bin/codesign -T /usr/bin/productsign -P "${P12PASS}" -A; then
  echo "ERROR: security import failed. Current identities:" >&2
  security find-identity -v -p codesigning >&2 || true
  echo "PKCS12 head (hex):" >&2
  xxd "${WORKDIR}/cert.p12" | head -3 >&2 || true
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
