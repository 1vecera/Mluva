# macOS source preview

Mluva's macOS client is available as source. No current public binary is separately verified, Developer ID signed, notarized or updateable.

## Requirements

- macOS 14 or newer and Xcode Command Line Tools with Swift.
- A local signing identity for stable local app bundles.
- Microphone access; Speech Recognition access for Apple Speech.
- Accessibility access for the global shortcut, insertion and optional typed snippets. Without it, completed text is copied to the clipboard.
- Screen Recording access only for system audio in Meeting mode. Mluva consumes audio and does not retain video.
- Application Default Credentials and a Speech-to-Text enabled Google Cloud project when using Google recognition.

## Local build

```bash
swift test
make setup-signing
make release
make install
```

`make setup-signing` verifies the existing self-signed `Voice Scribe Local Signing` identity in the login keychain. Mluva retains that identity so privacy grants survive the visual rename and rebuilds. If it is missing, the helper opens Keychain Access and prints the Certificate Assistant settings needed to create it. Set `SIGNING_IDENTITY` to another installed local identity when needed.

For Google recognition, configure Application Default Credentials:

```bash
gcloud auth application-default login
```

Select Google in Settings, allow cloud recognition and enter the project ID. The EU endpoint and Chirp 3 are the defaults. Mluva uses ADC unless you choose a service-account JSON; it stores only the path and does not copy the private key into preferences.

## Distribution packaging

`make distribution` requires an Apple Developer Program `Developer ID Application` identity. It enables the hardened runtime and microphone entitlement, timestamps the signature, submits to Apple's notary service, staples and validates the ticket, and assesses the result with Gatekeeper. The default `notarytool` profile is `voice-scribe`; override it with `NOTARY_PROFILE`.

CI uses `make ci-package` to validate the bundle shape with an ad-hoc signature. This mode requires `CI=true` and is separate from the local release targets. See the [brand and compatibility contract](brand-and-compatibility.md) for retained technical identifiers.
