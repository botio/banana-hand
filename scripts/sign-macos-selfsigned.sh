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

KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
security unlock-keychain -p "" "${KEYCHAIN}" >/dev/null 2>&1 || true

import_p12() {
  security import "$1" -k "${KEYCHAIN}" \
    -T /usr/bin/codesign -T /usr/bin/productsign -P "" -A
}

# LibreSSL's default PKCS12 MAC fails Apple's importer with "MAC verification
# failed". Try a SHA-256 MAC first; if that still fails, fall back to a
# MAC-less PKCS12 (the MAC is optional per PKCS#12, so there is nothing to
# verify).
openssl pkcs12 -export -out "${WORKDIR}/cert.p12" \
  -inkey "${WORKDIR}/key.pem" -in "${WORKDIR}/cert.pem" \
  -name "${CERT_NAME}" -password pass: -macalg SHA256

if ! import_p12 "${WORKDIR}/cert.p12"; then
  echo "SHA-256-MAC PKCS12 import failed; retrying with a MAC-less PKCS12..." >&2
  openssl pkcs12 -export -out "${WORKDIR}/cert-nomac.p12" \
    -inkey "${WORKDIR}/key.pem" -in "${WORKDIR}/cert.pem" \
    -name "${CERT_NAME}" -password pass: -nomac
  import_p12 "${WORKDIR}/cert-nomac.p12"
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
