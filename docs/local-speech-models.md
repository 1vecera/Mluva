# Local speech models for Mluva

The five-stop slider is ordered by model storage and estimated CPU working RAM. Qwen3-ASR 1.7B is the default local selection. CPU remains the portable default; NVIDIA acceleration is optional. Selecting a model does not download it. Continue and Apply wait for verified files and a supported recognition language.

| Slider choice | Model download | Estimated CPU RAM | Runtime |
| --- | ---: | ---: | --- |
| Whisper Tiny | 42 MB | 0.5 GB | ONNX |
| Whisper Base | 78 MB | 0.75 GB | ONNX |
| Whisper Small | 250 MB | 1.5 GB | ONNX |
| Parakeet TDT 0.6B v3 INT8 | 670 MB | 2 GB | ONNX |
| Qwen3-ASR 1.7B Q4_0, recommended | 1,585 MB | 3.5 GB | llama.cpp |

The RAM values include headroom; they are not guaranteed peaks or total computer requirements. Whisper Turbo remains a benchmark/compatibility catalog entry but is no longer a sixth slider stop. An old Turbo configuration reopens setup with Qwen selected and requires its download before continuing.

## What “live” means here

All these integrations process short audio chunks. They are **not native acoustic streaming engines**. Whisper and Parakeet publish completed chunk text. Qwen additionally streams provisional decoder tokens after each chunk has been encoded. Audio keeps arriving while inference runs; pending audio is coalesced instead of creating concurrent inference jobs. Stop recognizes the complete recording again to reconcile words across chunk boundaries. That adds measurable finalization time.

Qwen's official streaming implementation uses vLLM. This app uses the smaller llama.cpp runtime instead; upstream streaming latency claims do not apply. The Q4_0 conversion reduces storage and computation compared with Q8/BF16, with an unmeasured accuracy tradeoff. A successful sample transcription is not an accuracy evaluation.

## Ownership, memory and storage

Mluva owns weights and runtimes in its XDG data directory. It does not borrow another application's models, configuration, processes or endpoints. The retired provider adapter is removed. Only one-way settings cleanup and historical route labels remain so existing installations and saved records still open.

Weights load on the first audio chunk, remain available during that recording, and unload at Stop or Cancel. The process is not a persistent system service. CPU ONNX workers have a 5-billion-byte address-space limit; GPU and Qwen workers have a resident-memory watchdog. No local failure switches to a paid provider.

Managed models, runtimes and the Qwen shader cache count toward the 5-billion-byte download storage guard. The base app environment and temporary installation space are separate. All five CPU choices fit together. ONNX NVIDIA support adds approximately 3.36 GB, so installing every GPU model together would exceed the managed budget; download only the models needed. Tests use separate model stores for that reason.

Qwen uses pinned llama.cpp b11011 binaries: CPU or Vulkan for NVIDIA. Device discovery selects the NVIDIA device by name, rather than assuming Vulkan device zero is the dedicated GPU. The model is the pinned `getonit/Qwen3-ASR-1.7B-Q4_0-GGUF` conversion, with the upstream Q8 audio projector. Release and model sizes and SHA-256 hashes are verified before use. The Qwen runtime listens only on an ephemeral loopback port with a temporary random credential, disables its web UI, ignores inherited HTTP proxies and redirects, and inherits no cloud credentials. The process and credential are removed together.

The ONNX GPU runtime uses pinned ONNX Runtime 1.26 and CUDA 12 wheels with hashes. It verifies CUDA availability and disables runtime fallback. Whisper's merged quantized decoder requires disabling an incompatible graph optimization in that runtime.

## Sources

Research refreshed 17 September 2026 with Exa and primary maintainer documentation. These sources describe architectures and supported configurations, not independently measured laptop performance.

- [Qwen3-ASR implementation](https://github.com/QwenLM/Qwen3-ASR): supported languages, official streaming backend and models.
- [llama.cpp Qwen ASR support](https://github.com/ggml-org/llama.cpp/pull/19441) and [audio preprocessing fix](https://github.com/ggml-org/llama.cpp/pull/23073).
- [Pinned llama.cpp release](https://github.com/ggml-org/llama.cpp/releases/tag/b11011).
- [Q4 conversion](https://huggingface.co/getonit/Qwen3-ASR-1.7B-Q4_0-GGUF) and [upstream GGUF/projector](https://huggingface.co/ggml-org/Qwen3-ASR-1.7B-GGUF).
- [Parakeet v3 model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) and [ONNX ASR runtime](https://istupakov.github.io/onnx-asr/usage/).
- [Whisper models](https://github.com/openai/whisper) and [Turbo model card](https://huggingface.co/openai/whisper-large-v3-turbo).
- [Moonshine model catalog](https://moonshine-voice.readthedocs.io/en/stable/models/available-models/) and [Voxtral Realtime](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602): native-streaming alternatives considered, but not implemented in this compact multilingual slider.

## Paced microphone-path verification on this PC

All six catalog entries were downloaded through the production size/hash verification path and run on CPU and NVIDIA GPU. A public 15.05125-second sample was repeated twice and fed in 50 ms frames at microphone speed. Each run used a new worker, the production three-second preview threshold, and final full-recording recognition. No microphone or paid speech provider was used. Hardware: i7-12700H, 62 GiB RAM, RTX A1000 Laptop GPU with 4 GiB VRAM. Runs were sequential under ordinary desktop/development load; these are single-run observations, not a controlled performance distribution.

| Model | First text CPU / GPU | Stop-to-final CPU / GPU | Peak worker RAM CPU / GPU |
| --- | ---: | ---: | ---: |
| whisper-tiny | 17.9 / 7.5 s | 5.6 / 1.9 s | 0.33 / 0.95 GB |
| whisper-base | 6.0 / 6.2 s | 6.6 / 3.3 s | 0.47 / 1.02 GB |
| whisper-small | 9.0 / 9.5 s | 13.7 / 7.9 s | 0.97 / 1.31 GB |
| parakeet-v3 | 11.2 / 10.8 s | 5.2 / 5.0 s | 1.15 / 2.07 GB |
| qwen3-1.7b | 7.2 / 5.4 s | 28.3 / 5.4 s | 2.97 / 1.60 GB |
| whisper-turbo | 22.6 / 19.5 s | 40.6 / 32.4 s | 2.21 / 2.11 GB |

Qwen's GPU row uses a prepared shader cache but a fresh model process. With a new shader cache, first text took 19.9 seconds; Stop took 5.4 seconds. Peak total NVIDIA memory in the cached run was 2,284 MiB, including desktop usage. CPU Qwen needs no dedicated GPU, but its 28-second finalization makes the tradeoff visible. The CPU and GPU both produced updates while audio continued arriving.

Tiny's CPU first chunk repeatedly hallucinated “yeah”; the final full-recording transcript recovered. Its smallest download does not imply the best short-phrase behavior. Turbo produced only one update before Stop on either device, so it is retained only for comparison/compatibility. None of these tests establishes general accuracy, and all short chunks can cut words or lose context.

Every run confirmed no loaded worker at session creation and no remaining worker after finalization. Additional Qwen tests cancelled during loading and after preview text on both devices; cleanup took at most 1.23 seconds and discarded late text. Tests caught and fixed nullable streaming control messages and a concurrent cancellation cleanup race.

[Measurements and provisional/final output sizes](benchmarks/local-speech-2026-09-17.json) use only the [public Qwen sample](https://qianwen-res.oss-cn-beijing.aliyuncs.com/Qwen3-ASR-Repo/asr_en.wav). Reproduce with `scripts/benchmark_local_speech.py` after downloading a model into a task-specific XDG data directory; the script itself never downloads or records audio.
