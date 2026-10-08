# Private browser recorder

The optional `mluva-web` companion records audio on a phone, tablet or another computer, transfers it through Cloudflare Tunnel to the PC running Mluva, transcribes it with ElevenLabs Scribe v2 and copies completed text to that PC’s Wayland clipboard. It saves browser recordings in the same local History database as the native app. Open History or Latest conversation in Mluva to see new recordings. Authenticated browsers poll the same server for recent browser recordings; other desktop history is not exposed.

The PC serves the webpage and must remain online. This is a browser recording inbox, with final transcription after Stop. The native conversation editor and Live rewriting remain desktop features. No separate remote server, cloud transcript database or microphone access on the receiving PC is needed.

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

Keep the page open while recording. Stop transfers the completed recording, and the response starts a background job so transcription does not hold a Cloudflare request open. The page polls for the result. The browser saves untransferred audio in its own IndexedDB before upload; a failed transfer or expired login leaves Retry, Download audio and Discard local audio available. Reload restores an unfinished recording. Completed transfer clears that browser recovery copy. Audio captured before Stop is held in browser memory; closing the page, locking the device or operating-system interruption can lose an active recording. Screen wake-lock is requested where available, but is not a background-recording guarantee.

Uploads are limited to 20 MiB and 10 minutes. The server converts supported WebM/Opus, MP4, Ogg and WAV to mono 16 kHz PCM WAV, then uses the existing native Scribe provider client. The first version always uses ElevenLabs Scribe; the native app’s alternative speech-provider selection does not change the browser route. Provider credentials never reach the browser. The companion snapshots Mluva’s language, automatic-copy, audio-retention and Incognito settings at startup; restart it after changing those settings. Browser recordings do not generate titles or run rewrites automatically.

Completed recognition preserves raw text separately from the editable delivered version in Mluva History. Retried upload IDs return a completed durable receipt without sending the audio to Scribe again, including after companion restart. At most one recognition job runs at a time. Other uploads stay on their originating devices until retried. Clipboard failure leaves the transcription available; Copy to PC retries delivery without another transcription. Copy on this device uses that device’s browser clipboard permission. Browser polling does not automatically replace either device’s clipboard.

Normal server staging uses private temporary files and deletes them when processing ends. Mluva’s retention policy controls durable recovery WAV files under its recordings directory. Incognito uses the native memory-backed audio store and independent cleanup helper, writes no History or retained audio, and exposes no saved browser history. The existing installed `mluva-audio-cleanup` helper must be present. Browser-local recovery still exists until successful transfer or explicit discard, and cloud speech still sends audio to ElevenLabs. Incognito receipts are volatile and cannot deduplicate a retry after server restart.

The companion is opt-in and is built separately; native installation does not start a web listener or modify Cloudflare. It adds entries to the existing backward-compatible History schema. Stopping the companion and tunnel reverses runtime exposure; saved local recordings remain under Mluva’s ordinary deletion and retention controls.

## Verification

```sh
cargo test --locked -p mluva-web --target-dir tmp/native-build
cargo clippy --locked -p mluva-web --all-targets --target-dir tmp/native-build -- -D warnings
node --check rust/mluva-web/web/app.js
node --check rust/mluva-web/web/sw.js
```

Tests use synthetic JWT signing keys, a local Scribe peer, generated audio, disposable History and a clipboard peer that writes only into its temporary directory. The optional headless Chromium check exercises credentialed manifest/icon loading, service-worker control without response caches, installation guidance, recording, transfer, two-device history and local audio recovery across reload; it does not open a physical microphone or touch the host clipboard. Install Playwright under the checkout’s `tmp/browser`, then use the offscreen runner:

```sh
npm install --prefix tmp/browser --no-fund --no-audit playwright
MLUVA_WEB_BROWSER_DRIVER="$PWD/rust/mluva-web/tests/browser.mjs" \
  bash /path/to/run_isolated_x11.sh tmp/browser-isolated -- \
  cargo test --locked -p mluva-web --target-dir tmp/native-build \
  browser_record_stop_transfer_and_recovery -- --ignored --nocapture
```

The browser driver uses `/usr/bin/chromium` with synthetic microphone flags. Physical phone browsers, device locking and live Wayland paste behavior require device acceptance; isolated browser evidence does not establish those results.
