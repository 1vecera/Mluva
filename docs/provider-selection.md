# Choosing a speech provider

Open **Settings → Providers**. Choose Microsoft MAI, ElevenLabs, xAI Grok, Mistral, Local model or Custom. Microsoft MAI is listed first and labelled **Suggested**; an existing installation keeps the provider it saved. Changes save automatically for the next recording; active capture keeps its frozen settings. Incomplete or invalid choices stay visible for correction. Account login never changes this choice. Rewriting providers and model pickers have been removed.

| Provider | Setup |
| --- | --- |
| Microsoft MAI | MAI-Transcribe-2 through Azure Speech: the live `mai-transcribe-2-streaming` model, or the fast-transcription batch endpoint. Enter the Azure Speech key and the region of its resource below the choice. A Microsoft preview without an SLA. The free (F0) tier allows one live stream at a time; parallel sessions returned empty transcripts in testing. |
| ElevenLabs | Scribe v2 live recognition with explicit batch fallback, or batch only. Enter the key below its choice; leaving the key field or pressing Enter stores it in the desktop keyring. Existing environment-key aliases still work. Meeting uses ElevenLabs separately. |
| xAI Grok | Grok Voice Transcribe 2.0, live over a WebSocket or as a batch upload. Enter the xAI API key below its choice. |
| Mistral | Voxtral 3: Voxtral Mini Realtime (`voxtral-mini-transcribe-realtime-3`) for live text, Voxtral Small (`voxtral-small-transcribe-3`) for batch. Enter the Mistral API key below its choice. The entry pay-as-you-go tier answers bursts with HTTP 429: batch waits out `Retry-After` a few times, and a refused live connection reports it and recognizes the recording in batch. |
| Local model | Choose Whisper Tiny, Parakeet v3 or the default Qwen3-ASR 1.7B. Download verified weights into Mluva storage; CPU is supported, NVIDIA acceleration is optional. Audio stays on the PC with no cloud fallback. |
| Custom | Enter a LiteLLM/OpenAI-compatible transcription endpoint and deployment ID. The existing `whisper` alias is a suggestion, not a verified model. |

## Realtime or batch

Each cloud provider shows a **Realtime / Batch** choice with its price per audio hour, before taxes. The choice is `transcription_mode` and applies to whichever cloud provider is selected.

| Provider | Realtime | Batch |
| --- | --- | --- |
| Microsoft MAI | $0.54 | $0.10 (introductory price until 31 December 2026) |
| ElevenLabs | $0.39 | $0.22 |
| xAI Grok | $0.20 | $0.10 |
| Mistral | $0.48 | $0.24 (Voxtral Small; Mini batch at $0.21 is not offered) |

Prices were checked on 30 October 2026 at elevenlabs.io/pricing/api, microsoft.ai/models/mai-transcribe-2 and docs.x.ai/developers/pricing, and vendors change them. Realtime shows words while you speak; the committed text still comes only from finished segments. If a stream cannot start or finish, Mluva recognizes the retained recording in batch and says so in History and the capture status. A realtime stream that finishes without text for a recording of one second or more counts as failed, since a service that could not serve the stream (the Azure free tier with parallel sessions) can answer with an empty success; the recording is then recognized in batch, and a silent recording that long costs one extra batch request before it is reported as no speech. ElevenLabs keeps its released behavior and is not affected. Batch sends the whole recording at Stop, shows no live words and is not a fallback: nothing is labelled as one.

## Credentials

Keys entered in Settings go to the desktop keyring through `secret-tool`, one entry per provider, and are looked up before the environment. The attributes are `application mluva provider <id>`:

| Provider | Keyring attribute `provider` | Environment aliases, in order |
| --- | --- | --- |
| Microsoft MAI | `microsoft` | `AZURE_SPEECH_KEY`, `SPEECH_STUDIO_KEY` |
| ElevenLabs | `elevenlabs` | `ELEVENLABS_API_KEY`, `DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL`, `ELEVEN_LABS_STT_TOKEN` |
| xAI Grok | `xai` | `XAI_API_KEY`, `DAS_ITEM_GROK_STT_API_KEY__KEY` |
| Mistral | `mistral` | `MISTRAL_API_KEY`, `DAS_ITEM_MISTRAL_API_KEY__KEY` |

Store a key from a shell with `secret-tool store --label='Mluva Microsoft Azure Speech key' application mluva provider microsoft` (`--label='Mluva xAI API key' … provider xai`, `--label='Mluva Mistral API key' … provider mistral`), then paste the key on standard input. The Azure **region** is not secret: it is the `microsoft_region` setting, with `AZURE_SPEECH_REGION` used only when the setting is empty. Enter the region name, such as `swedencentral`; there is no default, because a wrong region fails as a rejected key. Streaming MAI-Transcribe-2 is served in `swedencentral`, `centralus`, `eastus2` and `southeastasia` (Microsoft documentation, October 2026). An existing managed launcher profile supplies the selected provider's key at startup, as it did for ElevenLabs (see the [Linux guide](../linux/README.md)). Keys never appear in settings, History, diagnostics or logs, and a provider's answer is never echoed in an error message.

Settings written by this build omit `transcription_mode` and `microsoft_region` until they differ from their defaults, so the previous release still reads the file until a new provider, mode or region is chosen.

## Why these providers

The Providers page has a short **Why these providers** section with the benchmark behind the suggestion: 62 public clips (Google FLEURS Czech and English, CC BY 4.0, plus 12 synthetic Czech and English technical sentences), word error rate in % where lower is better.

| Route | Czech | English | Czech technical | First text | After Stop | $ / audio hour |
| --- | --- | --- | --- | --- | --- | --- |
| Scribe v2 realtime | 10.8 | 4.1 | 14.8 | 2.2 s | 0.22 s | 0.39 |
| MAI-Transcribe-2 realtime | 5.3 | 4.4 | 22.2 | 0.4–1.3 s | 0.16 s | 0.54 |
| Grok realtime | 12.2 | 4.6 | 22.2 | 1.1 s | 0.30 s | 0.20 |
| Scribe v2 batch | 4.0 | 5.0 | 11.1 | – | 1–4 s request | 0.22 |
| MAI-Transcribe-2 batch | 4.2 | 3.4 | 22.2 | – | 0.6 s request | 0.10 |
| Grok batch | 9.9 | 4.6 | 23.5 | – | 0.7–2.5 s request | 0.10 |
| Mistral Voxtral realtime | 10.1 | 5.2 | 23.5 | 1.2–2.0 s | 0.66–0.92 s | 0.48 |
| Mistral Voxtral Small batch | 3.8 | 2.3 | 17.3 | – | 0.8–1.0 s request | 0.24 |

MAI realtime halves Scribe's realtime Czech error (95% confidence interval −9.1 to −2.2 points); English is a tie for the realtime routes. On Artificial Analysis' English batch leaderboard (AA-WER, October 2026) MAI-Transcribe-2 scores 2.0%, Scribe v2 2.2% and Grok 2.3%. MAI is weaker on Czech technical terms. Voxtral Small batch is the most accurate batch route in English (95% interval of its difference from Scribe batch −5.0 to −0.2 points) and ties Scribe and MAI in Czech; Voxtral Mini Transcribe 3 batch ($0.21) scored 4.0 / 2.5 / 18.5 and is not offered. Voxtral Mini Realtime is weaker in Czech than MAI realtime (interval +2.0 to +7.9 points) and finishes more slowly after Stop, so **Batch is the recommended mode for Mistral**; Realtime stays available. Mistral is EU-hosted. Alibaba Fun-ASR is not offered because its international sign-up is complex. These are a small public-clip benchmark, not a guarantee for your voice or vocabulary.

Custom model search and **Enter model ID…** work even when discovery is unavailable. Refresh reads metadata without audio or transcript text. Listings never silently replace the selected deployment and do not prove account access. Changing endpoint/key reference/provider or closing the page invalidates older replies.

Connection details store the API URL and credential environment-variable **name**, never its value. Include the server's prefix, usually `/v1`. Loopback may use HTTP; remote endpoints require HTTPS and reject embedded credentials, query parameters, fragments and redirects. Restart after changing launch environment credentials. Downloads start only when requested, not merely by opening Settings.

Local/compatible speech previews use the configured chunk size; shorter chunks may increase requests. Complete audio is recognized at Stop. The selected speech route, raw originals, exact-target delivery and retention remain independent of account sync.

See [managed models](local-speech-models.md), [onboarding](onboarding.md) and [verification](../CONTRIBUTING.md#verification); the [cloud provider evidence](verification/cloud-providers/README.md) records the live checks. The Microsoft MAI, xAI Grok and Mistral routes, like the local and custom routes, are Experimental until physical acceptance. The macOS app dictates with ElevenLabs only and ignores these choices.
