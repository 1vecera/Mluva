# Native local speech previews

The reference is unchanged Mluva 1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. An external collector verifies the released local/batch preview, local speech and asset modules against that commit and invokes their actual sessions and clients. [Released observations](released-local-preview.json) have SHA-256 `6c8e248f7464026ea64e26e4a5b10e03fe1d895a0e15bf9a0b782000a7b98a87`. Only model readiness and the inference process are supplied by sparse pinned files and controlled native peers; no Python comparison code enters the maintained tree.

The native comparison uses the production `LocalPreviewClient`, shared sequential preview owner and actual native local speech factory. Forty-four sessions compare 76 public preview/health/byte/process observations, complete final results, exact WAV hashes/sizes/private modes, ONNX requests and Qwen audio/JSON payloads. The three retained models each exercise CPU and GPU protocol routes. These controlled routes do not establish physical GPU inference; the separate linked inference evidence retains its scope.

| Released behavior | Checked boundary |
| --- | --- |
| Idle cancellation and below-threshold final recognition | Starting creates no process, model allocation or audio file; final recognition owns and releases one worker |
| Sequential previews and final reconciliation | One resident worker serves both chunks and the complete recording; provisional words never become authoritative final output |
| Rewriting paused and idle cancellation | Local speech previews remain enabled; cancellation also reaps a resident worker between calls |
| Preview failure and Stop during inference | Failed previews request finalized-recording fallback; Stop cancels an active worker and recognizes the complete audio with a fresh client |
| Qwen partial cancellation | The public partial appears while the peer has sent only its first SSE fragment and the private WAV remains present; cancellation clears text, reaps the worker and erases its temporary key/WAV |

The initial comparison exposed a test-boundary error: the synthetic peers used Linux `PDEATHSIG`, which follows the creating thread. The unchanged release starts its worker from the preview thread, so stopping that thread killed the synthetic model before final recognition. An external exception trace independently observed SIGKILL (`-9`) and connection refusal before the final HTTP request. The peers now rely on the actual recording's owned cleanup; the isolated reference runner also owns their PID namespace. Collecting the unchanged release again produces successful final reconciliation for every model/device reuse case. All native observations match that corrected collection, and the earlier 178 Qwen and 230 ONNX process/protocol regressions pass with the corrected peers.

```sh
cargo test --locked -p mluva-providers --test local_preview --test batch_preview
```

[Joined capture evidence](../../../mluva-gtk/tests/fixtures/capture-lifecycle-evidence.md) additionally checks actual GTK captures through compatible and local preview sessions. Complete service/application assembly, realtime segment production, Live scheduling/revision/final reconciliation, pending screenshot pickers, physical microphone acceptance and whole-app performance remain required. All complete-workflow rows remain Pending; the installed app/widget remain 1.6.0 and the full Rust goal stays active.
