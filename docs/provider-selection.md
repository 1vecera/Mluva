# Choosing speech and rewrite providers

Open **Settings → Providers**. Use the visible buttons to choose speech recognition and optional polishing. Only Custom exposes endpoint and model details. **Apply** saves the page for the next recording or request. Recording and rewriting must be idle before applying changes. Switching providers keeps the other providers’ model choices; opening Settings again reloads the saved configuration. Workspace/Live controls remain on their existing surfaces.

First launch uses the [three-step onboarding](onboarding.md), including optional rewriting and a live appearance preview.

| Task | Provider | Default and setup |
| --- | --- | --- |
| Speech | ElevenLabs | Scribe v2. Paste the API key directly below the ElevenLabs choice; Apply stores it in the desktop keyring. A saved key takes priority over environment keys, whose supported legacy aliases still work. Meeting continues to use ElevenLabs. |
| Speech | Local model | Mluva downloads verified weights into its own storage. Choose one of five RAM/storage sizes, optionally enable NVIDIA GPU, and press Download model. Five models support Czech and English; no other app is required. |
| Speech | Compatible API | Use a transcription deployment on a LiteLLM or OpenAI-compatible server. The existing `whisper` alias remains the initial suggestion; replace it with your server’s deployment ID. |
| Rewrite | Skip | Keep transcription and editing; disable polish, Live rewrite and generated titles. |
| Rewrite | Codex | Use the existing Codex account and selected model. Authenticate through `codex login`; the compact workspace rewrite picker retains model and supported Fast-tier choices. |
| Rewrite | Compatible API | Choose a chat deployment on a LiteLLM or OpenAI-compatible server. A model ID is required; Mluva does not guess a cloud model or silently select the first catalog entry. |

Custom model choices support search. **Enter model ID…** allows an explicit alias when the catalog is unavailable or omits a deployment. A listed model is not proof that the account can use it, and a model absent from a compatible server’s listing may still work. Codex follows its existing configuration. Local models use a size slider; see [model choices](local-speech-models.md).

**Refresh** is optional for Codex and compatible APIs. It reads model metadata without sending audio or text. The local model slider only previews requirements; Download model starts installation, opening settings alone does not download models. Speech and rewrite endpoints retain independent catalogs.

A failed, empty or malformed listing leaves the current choice usable and explains the next step. Editing the endpoint/key reference, changing providers, closing the page or reopening Settings invalidates any older in-flight result. Discovery runs off the GTK thread, with bounded requests and child cleanup. It never falls back to another URL or provider.

**Connection details** appears only for compatible APIs. Expand it to edit the API base URL and the **name** of the key environment variable. Include the server’s API prefix, often `/v1`. Speech and rewrite retain independent endpoint/key references. Local servers may not require a key; remote servers generally do. Mluva reports whether a named key exists in its own process environment without showing its value or claiming account access. After changing the launch environment, restart Mluva. URLs reject embedded credentials, query parameters and fragments; remote connections require HTTPS. No credentials or fetched catalogs are saved to `config.json` or diagnostic logs.

**Live speech previews** contains the existing chunk-size option for batch speech providers. Shorter chunks can update sooner and increase requests; the complete recording is recognized again at Stop. Provider settings do not change capture scheduling, raw recognition, clipboard delivery or audio retention.

The compact rewrite picker continues to select models for the current rewrite provider. Switching endpoints clears its old catalog and loading state. Compatible servers have no synthetic “Default” selection that could erase a required deployment alias.

## Troubleshooting

If discovery fails, keep the saved choice or enter the model ID manually. Confirm that the selected endpoint serves the right task and that its key variable is present in Mluva's process. Restart the app after changing credentials in the launch environment.

Provider transport and UI checks are documented in [Contributing](../CONTRIBUTING.md#verification). Compatible API and local Whisper routes remain Experimental; a successful catalog request does not establish recognition quality or account access.
