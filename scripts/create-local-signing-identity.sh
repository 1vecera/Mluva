#!/bin/bash
set -euo pipefail

IDENTITY_NAME="Voice Scribe Local Signing"

has_signing_identity() {
    security find-identity -v -p codesigning \
        | awk -F'"' -v name="$IDENTITY_NAME" \
            '$2 == name { found = 1 } END { exit !found }'
}

if has_signing_identity; then
    echo "Local signing identity is already available: $IDENTITY_NAME"
    exit 0
fi

open -a "Keychain Access"

cat >&2 <<EOF
No usable '$IDENTITY_NAME' code-signing identity was found.

Keychain Access has been opened. In Certificate Assistant, create a certificate with:

  Name:             $IDENTITY_NAME
  Identity type:    Self Signed Root
  Certificate type: Code Signing
  Validity:         3650 days
  Key usage:        Signature
  Extended usage:   Code Signing
  Keychain:         login

Then open the certificate's Trust section and set Code Signing to Always Trust.
Run 'make setup-signing' again to verify the completed identity.
EOF

exit 1
