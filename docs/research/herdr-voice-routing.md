# Herdr voice routing: research and integration notes

Research date: 16 October 2026. This supports the [portable feature spec](../../tasks/herdr-voice-routing-and-spoken-replies.md). No live agent prompt, terminal read, focus change, paid synthesis or Herdr upgrade was performed for this research.

## Recommended product boundary

Mluva should own capture, recipient selection, the cancellation window, delivery feedback and optional spoken summaries. Herdr should own agent discovery and transport. Daniel's separately configured chief-of-staff agent can own planning, memory and delegation. That division keeps direct dictation useful now and allows the same recipient picker to address a coordinator later.

This recommendation is an inference from established patterns, not evidence that most people already use a single chief of staff. Anthropic's [chief-of-staff cookbook](https://platform.claude.com/cookbook/claude-agent-sdk-01-the-chief-of-staff-agent) demonstrates a coordinator using persistent context, specialized subagents and permission controls. Its [orchestrator-workers guidance](https://www.anthropic.com/engineering/building-effective-agents) describes a central model delegating work and synthesizing results. Neither requires embedding another autonomous manager inside a dictation client.

## Interaction contract

| Moment | Expected behavior |
| --- | --- |
| Choose recipient | Show the Herdr name with agent type, project and machine/session context; resolve actual identity. |
| Hold shortcut | Freeze recipient, stop any reply audio, begin native capture. Pauses do not submit. |
| Release | Stop capture and finalize recognition; cancellation remains available. |
| Final text ready | Start the full two-second countdown with text, recipient, Cancel and Keep draft visible. |
| Countdown ends | Atomically validate the intended agent and submit once through Herdr. |
| Accepted / uncertain | Distinguish confirmed receipt from an ambiguous timeout; never automatically resend ambiguous text. |
| Reply or status | Show text; optionally speak one grounded sentence, scoped to the intended agent and message. |

Editing, switching recipient, starting another capture or losing the session disarms a pending send. A send already accepted cannot be recalled by the cancellation control. A late reply from the previous selection must retain its attribution and must not interrupt a new recording.

## What the installed Herdr actually supports

Read-only introspection on Lenovo reported Herdr **0.9.1**, API schema version **1**, protocol **22**. Reproduce without reading conversations:

```sh
herdr --version
herdr api schema --json
herdr agent prompt --help
```

The installed CLI provides named-agent discovery, prompt submission, state inspection and event subscriptions. `agent prompt` submits text with Enter; it is not just a paste operation. Metadata includes the agent's native session identity, terminal/pane/workspace identifiers, revision and completion sequence. Names are scoped to a server, and pane identifiers alone cannot distinguish a replacement agent.

The installed `AgentPromptParams` contains `target`, `text` and optional `wait`; it has **no expected-occupant condition or caller submission key**. Its response reports agent state rather than a durable per-message receipt. Consequently, “look up the name, check it, then send” leaves a race, and retrying a timeout can duplicate a real submission. The implementation must close that gap through an appropriate supported capability or a small Herdr protocol extension before claiming safe automatic routing. A request ID by itself does not provide that guarantee.

`--wait` is not tied to an individual prompt. When an agent is already working, completion of its earlier turn may satisfy the wait. A timeout or stalled response can occur after text was sent. Use correlated native turn evidence for a result summary; otherwise report current activity without claiming the dictated request completed. The blocked state must remain a permission boundary.

Subscriptions are useful for availability and state updates but are not a durable delivery journal. Reconnect needs an authoritative snapshot and reconciliation, including selected-session identity. A server/socket restart invalidates pending sends. Keep a bounded local attempt record under the applicable retention rules; ambiguous attempts remain manual recovery, not an automatic replay queue.

Public documentation was at **v0.9.5**, newer than Lenovo's installation: [agent automation](https://raw.githubusercontent.com/herdrdev/herdr/v0.9.5/docs/next/website/src/content/docs/agent-automation.mdx) and [socket API](https://raw.githubusercontent.com/herdrdev/herdr/v0.9.5/docs/next/website/src/content/docs/socket-api.mdx). Pin the actual integration version and capability-check it. CLI session/socket resolution is independent of whichever remote machine is selected visually in the TUI. Some history-reading modes can scroll the terminal; prefer passive events or native replies and never move Daniel's viewport to build a spoken summary.

## ElevenLabs v4 Turbo

The requested model exists as **`eleven_v4_turbo`**. ElevenLabs lists it as real-time TTS through its Text-to-Dialogue WebSocket. The advertised roughly 100 ms median is provider inference latency, excluding network and application time; it is not a measured Mluva response time. [Official models reference](https://elevenlabs.io/docs/overview/models).

Use `wss://api.elevenlabs.io/v1/text-to-dialogue/stream-input` with that model ID, not a substituted legacy TTS endpoint. Register one voice per connection and stream the sentence as dialogue input. Short replies need an explicit flush (or closing flush) so buffering does not withhold speech. Authentication needs TTS permission; prefer the credential header to query-string secrets. The documented idle timeout is 20 seconds. [Official streaming guide](https://elevenlabs.io/docs/eleven-api/guides/how-to/websockets/realtime-tdd).

Product implications: synthesize on demand, use one configured voice, bound each reply, and keep text available on authentication, network or audio failure. A summarizer, if needed, receives only the relevant bounded agent response under an explicit provider choice; ElevenLabs receives only the resulting sentence. Never send an entire terminal buffer to either service by default. Do not pad a short reply to trigger synthesis. Report measured recognition, deliberate grace, agent response and playback delays separately. This research made no paid requests and established no end-to-end latency result.

## Implementation questions already resolved by repository evidence

- `rust/mluva-gtk/src/global_shortcuts/session.rs` handles shortcut deactivation as debounce cleanup; `application/shortcuts.rs` connects activation to the existing toggle. Preserve that binding while adding actual hold/release behavior for agent mode.
- Native local capture and account sync are separate concerns. This feature does not need an account or hosted microphone transcription; choosing a voice must not change recognition providers.
- A normal web page cannot use the local Herdr Unix socket or a system-wide hold shortcut. The first delivery is native Linux; any future browser connection needs a separately designed, authenticated integration.
- There is no permission to exercise active personal agent sessions for proof. Use disposable sessions and synthetic text, then identify physical-key and real-audio evidence that remains unavailable.

Review focus: prove target identity at submission, cancellation at the boundary, one accepted send despite uncertainty, correct attribution of busy-agent replies, and silence during the next recording. These are the central risks; a polished dropdown alone is not completion.
