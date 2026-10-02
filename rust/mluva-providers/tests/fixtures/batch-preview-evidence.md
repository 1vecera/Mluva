# Native batch previews

The reference is unchanged Mluva 1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. The external collector verifies the released `batch_preview.py` bytes and invokes its actual session with synthetic PCM and real loopback compatible HTTP. [Released observations](released-batch-preview.json) have SHA-256 `e2b5d34ba82dc9bfc82369b1b4088640a18ec1804719ec53257bdbb5570d19ea`. The session uses the released speech client; only the endpoint/account is replaced. No provider account, real microphone, managed credential or user content is used.

The native test constructs the production `BatchPreviewClient` and actual compatible speech clients. Ten released sessions compare nineteen public preview/health/accepted-byte observations, complete final results, factory call counts and twelve independently observed multipart uploads. Uploaded WAV lengths and SHA-256 values match exactly; model/language fields match. The cases cover below-threshold audio, provisional chunk text, sequential/coalesced chunks, pause/resume, provider failure, cancellation followed by toggling, empty audio, odd PCM bytes, the exact 30-minute memory bound and overflow.

Starting a session loads no provider or model and creates no file. The microphone-facing call only appends bounded PCM; one background owner handles sequential inference. Paused audio remains available and resumes as one coalesced request. Stop cancels pending preview work and recognizes the entire recording once; preview text cannot become authoritative final text. Failure permits the caller's finalized-recording fallback. Cancellation erases queued audio and invalidates late text.

Two additional native faults synchronize on real receipt of a delayed HTTP request: cancellation of an active preview and dropping a pending final request. They verify one provider creation/upload, 0700 staging directories, a 0600 WAV, empty private staging after cleanup acknowledgement, unavailable health and no late preview text. Blocking staging and provider cleanup belong to runtime tasks that outlive dropped callers. Cancellation registers against the same state lock as new requests and awaits every owned request before acknowledging cleanup.

```sh
cargo test --locked -p mluva-providers --test batch_preview
```

This is compatible-provider batch-preview component evidence. Capture-controller integration, the separate local preview subclass with model reuse/Qwen partial callbacks, Live scheduling/revision/final reconciliation, pending-picker behavior and complete application/service assembly remain required. It establishes neither physical microphone acceptance nor whole-app performance. All complete-workflow acceptance rows remain Pending; the installed app/widget remain 1.6.0.
