# Choosing a speech provider

Open **Settings → Providers**. Choose ElevenLabs, Local model or Custom. Apply saves the next recording's route; active capture keeps its frozen settings. Account login never changes this choice. Rewriting providers and model pickers have been removed.

| Provider | Setup |
| --- | --- |
| ElevenLabs | Scribe v2 live recognition with explicit batch fallback. Enter the key below its choice; Apply stores it in the desktop keyring. Existing environment-key aliases still work. Meeting uses ElevenLabs separately. |
| Local model | Choose Whisper Tiny, Parakeet v3 or the default Qwen3-ASR 1.7B. Download verified weights into Mluva storage; CPU is supported, NVIDIA acceleration is optional. Audio stays on the PC with no cloud fallback. |
| Custom | Enter a LiteLLM/OpenAI-compatible transcription endpoint and deployment ID. The existing `whisper` alias is a suggestion, not a verified model. |

Custom model search and **Enter model ID…** work even when discovery is unavailable. Refresh reads metadata without audio or transcript text. Listings never silently replace the selected deployment and do not prove account access. Changing endpoint/key reference/provider or closing the page invalidates older replies.

Connection details store the API URL and credential environment-variable **name**, never its value. Include the server's prefix, usually `/v1`. Loopback may use HTTP; remote endpoints require HTTPS and reject embedded credentials, query parameters, fragments and redirects. Restart after changing launch environment credentials. Downloads start only when requested, not merely by opening Settings.

Local/compatible speech previews use the configured chunk size; shorter chunks may increase requests. Complete audio is recognized at Stop. The selected speech route, raw originals, exact-target delivery and retention remain independent of account sync.

See [managed models](local-speech-models.md), [onboarding](onboarding.md) and [verification](../CONTRIBUTING.md#verification). Local/custom routes retain their Experimental status until physical acceptance.
