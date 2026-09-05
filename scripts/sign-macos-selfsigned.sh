#!/usr/bin/env bash
# Generate a self-signed Apple code-signing certificate and import it into a
# local keychain, then print the codesign identity name to stdout.
#
# WHY a self-signed cert instead of ad-hoc ("-"):
# macOS 15+/26 (Tahoe) classifies a *quarantined, ad-hoc-signed* app as
# "damaged and can't be opened" — that dialog has no "Open Anyway" path.
# A *real* X.509 certificate (even one from an untrusted/self CA) is instead
# classified as "from an unidentified developer," which DOES expose the
# System Settings ▸ Privacy & Security ▸ "Open Anyway" button. This lets a
# non-notarized app be approved without a paid Apple Developer ID.
#
# The key is generated fresh on each CI runner (no secret stored), so the
# cdhash changes per build; users may need to re-approve after an update.
set -euo pipefail

CERT_NAME="Banana Hand Self-Signed Dev"
WORKDIR="$(mktemp -d /tmp/bh-selfsigned.XXXXXX)"
trap 'rm -rf "${WORKDIR}"' EXIT

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

# 3. Bundle key+cert into an empty-password PKCS#12.
openssl pkcs12 -export -out "${WORKDIR}/cert.p12" \
  -inkey "${WORKDIR}/key.pem" -in "${WORKDIR}/cert.pem" \
  -name "${CERT_NAME}" -password pass:

# 4. Import into the login keychain and grant codesign access to the key.
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
security unlock-keychain -p "" "${KEYCHAIN}" >/dev/null 2>&1 || true
security import "${WORKDIR}/cert.p12" \
  -k "${KEYCHAIN}" -T /usr/bin/codesign -T /usr/bin/productsign -P "" -A
security set-key-partition-list -S "apple-tool:,apple:" -k "" -A "${KEYCHAIN}" >/dev/null 2>&1 || true

echo "--- available codesign identities ---"
security find-identity -v -p codesigning || true
echo "--- selected identity ---"
echo "${CERT_NAME}"
