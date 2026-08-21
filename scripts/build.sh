#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_NAME="Mluva"
BUNDLE_NAME="VoiceScribeMac"
BUILD_DIR="$PROJECT_DIR/build"
APP_DIR="$BUILD_DIR/$APP_NAME.app"
ARCHIVE_PATH="$BUILD_DIR/$APP_NAME.zip"
ENTITLEMENTS_PATH="$PROJECT_DIR/Resources/VoiceScribeMac.entitlements"
SIGNING_MODE="${SIGNING_MODE:-local}"
LOCAL_SIGNING_IDENTITY="${LOCAL_SIGNING_IDENTITY:-Voice Scribe Local Signing}"

find_signing_identity_with_prefix() {
    local identity_prefix="$1"

    security find-identity -v -p codesigning \
        | awk -F'"' -v prefix="$identity_prefix" \
            'index($2, prefix) == 1 { print $2; exit }'
}

has_signing_identity() {
    local identity_name="$1"

    security find-identity -v -p codesigning \
        | awk -F'"' -v name="$identity_name" \
            '$2 == name { found = 1 } END { exit !found }'
}

require_apple_signing_identity() {
    local identity_prefix="$1"
    local identity="${SIGNING_IDENTITY:-}"

    if [[ -z "$identity" ]]; then
        identity="$(find_signing_identity_with_prefix "$identity_prefix")"
    fi

    if [[ -z "$identity" ]]; then
        echo "Error: no ${identity_prefix% } signing identity is available in the keychain." >&2
        echo "Create the certificate in the Apple Developer account, install it with its private key, then retry." >&2
        exit 1
    fi

    if [[ "$identity" != "$identity_prefix"* ]]; then
        echo "Error: SIGNING_IDENTITY must begin with '$identity_prefix' for SIGNING_MODE=$SIGNING_MODE." >&2
        exit 1
    fi

    printf '%s' "$identity"
}

case "$SIGNING_MODE" in
    local)
        SIGNING_IDENTITY="${SIGNING_IDENTITY:-$LOCAL_SIGNING_IDENTITY}"
        if ! has_signing_identity "$SIGNING_IDENTITY"; then
            echo "Error: local signing identity '$SIGNING_IDENTITY' is not available in the keychain." >&2
            echo "Run 'make setup-signing' once, then retry." >&2
            exit 1
        fi
        ;;
    distribution)
        SIGNING_IDENTITY="$(require_apple_signing_identity "Developer ID Application: ")"
        NOTARY_PROFILE="${NOTARY_PROFILE:-voice-scribe}"
        ;;
    ci)
        if [[ "${CI:-}" != "true" ]]; then
            echo "Error: ad-hoc CI packaging is only allowed when CI=true." >&2
            exit 1
        fi
        SIGNING_IDENTITY="-"
        ;;
    *)
        echo "Error: SIGNING_MODE must be local, distribution, or ci." >&2
        exit 1
        ;;
esac

cd "$PROJECT_DIR"

echo "Building $APP_NAME..."
swift build -c release --disable-dependency-cache

echo "Creating app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

cp ".build/release/$BUNDLE_NAME" "$APP_DIR/Contents/MacOS/"
cp "Resources/Info.plist" "$APP_DIR/Contents/"

# Strip extended attributes (iCloud Drive adds resource forks that break codesign)
xattr -cr "$APP_DIR" 2>/dev/null || true

echo "Signing with $SIGNING_IDENTITY..."
codesign_arguments=(
    --force
    --options runtime
    --entitlements "$ENTITLEMENTS_PATH"
    --sign "$SIGNING_IDENTITY"
)

if [[ "$SIGNING_MODE" == "distribution" ]]; then
    codesign_arguments+=(--timestamp)
fi

codesign "${codesign_arguments[@]}" "$APP_DIR"
codesign --verify --deep --strict --verbose=2 "$APP_DIR"

if [[ "$SIGNING_MODE" == "distribution" ]]; then
    echo "Submitting for notarization with keychain profile '$NOTARY_PROFILE'..."
    rm -f "$ARCHIVE_PATH"
    ditto -c -k --keepParent "$APP_DIR" "$ARCHIVE_PATH"
    xcrun notarytool submit "$ARCHIVE_PATH" \
        --keychain-profile "$NOTARY_PROFILE" \
        --wait
    xcrun stapler staple "$APP_DIR"
    xcrun stapler validate "$APP_DIR"
    spctl --assess --type execute --verbose=2 "$APP_DIR"

    # The submitted archive predates stapling; recreate the distributable with its ticket.
    rm -f "$ARCHIVE_PATH"
    ditto -c -k --keepParent "$APP_DIR" "$ARCHIVE_PATH"
fi

echo ""
echo "Built: $APP_DIR"
if [[ "$SIGNING_MODE" == "distribution" ]]; then
    echo "Archive: $ARCHIVE_PATH"
fi
echo "Run:     open \"$APP_DIR\""
echo "Install: make install"
