# Private browser recorder

The optional `mluva-web` companion lets an iPhone or another browser act as Mluva’s microphone through Cloudflare Access and Tunnel. **Live microphone on PC** is on by default: Record starts the native PC recording session, its recording widget appears automatically, and the browser streams half-second audio chunks while you speak. The PC microphone stays closed. Live words appear in the native widget and phone preview; only completed text is copied. Stop on either device ends the same native session.

Live mode uses the PC’s selected speech provider, language, rewrite/Live preferences, History, automatic-copy and retention settings, frozen for that recording. ElevenLabs Scribe uses the existing realtime connection and explicit batch fallback; other providers keep their ordinary native preview and Stop behavior. The native conversation editor remains on the PC. Phone capture is Dictation with clipboard delivery; it does not inspect or restore another app’s text target.

Turn Live microphone off to use the original recording inbox: audio is recorded on the phone, sent at Stop, transcribed with ElevenLabs Scribe v2 and copied to the PC. Both modes identify browser recordings in the same local History; authenticated devices see recent browser recordings without exposing unrelated desktop History. The PC must remain online. No separate remote server or cloud transcript database is needed.

The separate [hosted companion](../hosted/README.md) prepares account-based cross-platform device streaming and cloud text history. This private recorder continues using one native PC, its local History and its ordinary providers. The hosted companion does not migrate that History or alter this recorder’s access policy.

## Install on your phone

Open the protected recorder URL and sign in first. The page includes an Install app action. In Chrome on Android it opens the browser’s installation prompt when available; otherwise it shows the browser-menu instructions. On iPhone or iPad, open the URL in Safari, choose Share → Add to Home Screen, leave Open as Web App enabled if shown, then Add. Launch the Mluva icon next time. The installed app may need its own initial sign-in and microphone permission. The app opens directly to the recorder with one large Record / Stop control; recent recordings and longer help are collapsed below the result.

The manifest requests standalone display with the existing Mluva app mark. The manifest link sends credentials so it can load behind Cloudflare Access. Every manifest, icon and service-worker route uses the same signed-token perimeter as the page and API. The service worker forwards requests to the network without caching pages, transcripts, audio or login responses. The installed app needs an online PC and an unexpired sign-in; it does not add background recording or promise offline launches. Browser-local unfinished audio recovery still works after reconnection.

## Run

Install `ffmpeg` and `wl-clipboard`, then build the optional companion:

```sh
cargo build --locked -p mluva-web --release --target-dir tmp/native-build
```

Create a Cloudflare Access self-hosted application for the recorder hostname with an allow policy for your email. Configure a named Cloudflare Tunnel with that hostname pointing to `http://127.0.0.1:8787`, followed by a catch-all `http_status:404` ingress rule. Require Access protection before creating its public DNS record. Keep the tunnel token in your secret-manager launch environment as `TUNNEL_TOKEN`, never in an argument, config file or source checkout.

Supply the nonsecret origin, Access team URL and application audience, and resolve the existing ElevenLabs credential through the same managed environment or desktop keyring as Mluva:

```sh
export MLUVA_WEB_ORIGIN=https://mluva.example.com
export MLUVA_WEB_ACCESS_TEAM=https://your-team.cloudflareaccess.com
export MLUVA_WEB_ACCESS_AUDIENCE=your-access-application-audience
tmp/native-build/release/mluva-web
```

Run `cloudflared tunnel run` with `TUNNEL_TOKEN` in its environment. The origin binds only to loopback and verifies the signed Access JWT, issuer, audience, expiration and application-token type on every page, asset and API route. Changing requests also require the exact recorder Origin. There is no unauthenticated asset fallback, public share link or new app password. Sign in through Cloudflare on each device, then allow that device’s microphone when you first record.

## Recording and recovery

Keep the page open and unlocked while recording. In Live mode, Web Audio’s authenticated AudioWorklet resamples the actual microphone rate to 16 kHz, 16-bit little-endian mono PCM. Half-second requests are numbered and acknowledged in order; a retry of an acknowledged chunk cannot append it twice. The desktop admits one recording, rejects missing chunks, limits PCM to two hours, and cancels without final delivery after twenty seconds without audio. A slow connection stops the phone and preserves its compressed backup; it does not silently skip speech. The phone polls native state so PC Stop also stops phone capture. Native startup uses `mluva --phone-background` and does not present or focus the main window.

The bridge is an owner-only socket at `$XDG_RUNTIME_DIR/mluva/phone.sock`, with same-user peer checks and bounded requests. It opens no network listener. Only the authenticated companion crosses this socket; provider credentials stay on the PC. Native installation includes the bridge, while the companion and Cloudflare remain opt-in.

In inbox mode, Stop transfers the completed recording, and the response starts a background job so transcription does not hold a Cloudflare request open. The page polls for the result. The browser saves untransferred audio in its own IndexedDB before upload; a failed transfer or expired login leaves Retry, Download audio and Discard local audio available. Reload restores an unfinished recording. Completed transfer clears that browser recovery copy. Audio captured before Stop is held in browser memory; closing the page, locking the device or operating-system interruption can lose an active recording. Screen wake-lock is requested where available, but is not a background-recording guarantee.

Recordings are limited to 2 hours and uploads to 90 MiB, below [Cloudflare’s 100 MB request limit](https://developers.cloudflare.com/cache/concepts/default-cache-behavior/#upload-limits). The browser requests mono audio at 48 kbit/s (about 43 MB for two hours) and stops at 89 MiB to leave room for the codec’s final chunk. Browsers may choose a different bitrate; the byte limit remains a separate safety bound. The server converts supported WebM/Opus, MP4, Ogg and WAV to mono 16 kHz PCM WAV. It allows one second of final-frame/timer grace and rejects longer recordings before transcription, with conversion bounded to 2 hours plus 2 seconds. Conversion has a five-minute timeout; the Scribe connection/read timeout is thirty minutes. Transcription remains a background job polled by the browser.

The inbox route always uses ElevenLabs Scribe; Live microphone follows the native provider selection. Provider credentials never reach the browser. The companion snapshots Mluva’s language, automatic-copy, audio-retention and Incognito settings at startup; restart it after changing those settings. Inbox recordings do not generate titles or run rewrites automatically. Live recordings follow native title and rewrite preferences.

Completed recognition preserves raw text separately from the editable delivered version in Mluva History. Retried upload IDs return a completed durable receipt without sending the audio to Scribe again, including after companion restart. At most one inbox recognition job runs at a time; the native owner independently admits one recording. Other uploads stay on their originating devices until retried. Clipboard failure leaves the transcription available; Copy to PC retries delivery without another transcription. Copy on this device uses that device’s browser clipboard permission. Browser polling does not automatically replace either device’s clipboard.

Normal server staging uses private temporary files and deletes them when processing ends. Mluva’s retention policy controls durable recovery WAV files under its recordings directory. Incognito uses the native memory-backed audio store and independent cleanup helper, writes no History or retained audio, and exposes no saved browser history. The existing installed `mluva-audio-cleanup` helper must be present. Browser-local recovery still exists until successful transfer or explicit discard, and cloud speech still sends audio to ElevenLabs. Incognito receipts are volatile and cannot deduplicate a retry after server restart. Live Incognito uses the native recording’s private setting; an interrupted private session offers download/discard instead of falling back to an inbox whose startup settings might differ. Active Live sessions cannot be uploaded again through the inbox. Retry consults the native result first, so an uncertain connection never starts a second transcription automatically.

The companion is opt-in and is built separately; native installation does not start a web listener or modify Cloudflare. It adds entries to the existing backward-compatible History schema. Stopping the companion and tunnel reverses runtime exposure; saved local recordings remain under Mluva’s ordinary deletion and retention controls.

## Verification

```sh
cargo test --locked -p mluva-web --target-dir tmp/native-build
cargo clippy --locked -p mluva-web --all-targets --target-dir tmp/native-build -- -D warnings
node --check rust/mluva-web/web/app.js
node --check rust/mluva-web/web/sw.js
node rust/mluva-web/tests/pcm-worklet.mjs
bash linux/tests/run_application_smoke.sh phone
```

Tests use synthetic JWT signing keys, a local Scribe peer, generated audio, disposable History and a clipboard peer that writes only into its temporary directory. The optional headless Chromium check exercises credentialed manifest/icon loading, service-worker control without response caches, installation guidance, recording, real worklet PCM, preview before Stop, phone and PC Stop, two-device history and local audio recovery across reload; it does not open a physical microphone or touch the host clipboard. Install Playwright under the checkout’s `tmp/browser`, then use the offscreen runner:

```sh
npm install --prefix tmp/browser --no-fund --no-audit playwright
MLUVA_WEB_BROWSER_DRIVER="$PWD/rust/mluva-web/tests/browser.mjs" \
  bash /path/to/run_isolated_x11.sh tmp/browser-isolated -- \
  cargo test --locked -p mluva-web --target-dir tmp/native-build \
  browser_record_stop_transfer_and_recovery -- --ignored --nocapture
```

The browser driver uses `/usr/bin/chromium` with synthetic microphone flags. Physical phone browsers, device locking and live Wayland paste behavior require device acceptance; isolated browser evidence does not establish those results.

The private desktop phone check uses an independent IPC process and the actual GTK application, D-Bus widget signals, SQLite and isolated clipboard. It proves the native recording/level lifecycle, ordered PCM, duplicate rejection, phone/PC Stop, retention, Incognito, disconnect cancellation and shutdown while a synthetic PC microphone executable fails if invoked. It uses a local batch speech peer; the existing provider tests cover Scribe’s realtime protocol. This is private X11 evidence, not physical iPhone or live Wayland acceptance.
