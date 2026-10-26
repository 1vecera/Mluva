# First-run setup

Mluva has two steps: choose speech recognition, then preview recorder appearance. There is no rewrite account, prompt or Polish step.

Choose ElevenLabs with a key, download one of the three local models, or enter a compatible speech endpoint. Readiness is checked before capture. Existing installations retain their chosen speech provider, model, privacy and audio-retention settings. Login is optional and cannot change the recognition route.

The appearance preview lets you choose position, visible lines and opacity. Copy and Save stay visible in the editor. Smooth following is automatic and respects reduced motion. Finish opens ordinary dictation. Setup can be reopened through Settings.

For agent delivery, enable agent dictation in Settings → Capture and choose a Claude Code, Codex or Claude Desktop session in the recipient control. See [talking to an agent](agent-routing.md) for routes, statuses and cancellation. Account sync has its own toolbar icon and browser sign-in; it is not required for local recognition.

Verification: `make linux-provider-settings-test linux-application-test`; managed download/readiness checks remain in the provider suite. Physical microphone quality and desktop permission acceptance require the actual device.
