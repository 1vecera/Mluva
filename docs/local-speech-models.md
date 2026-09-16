# Local speech models for Mluva

Research checked 16 September 2026 using Exa: 28 search results reviewed, followed by primary model cards, maintainer repositories and runtime documentation. Vendor accuracy/latency figures are claims under their own test conditions, not a common benchmark or a promise about a laptop.

## Five candidates to choose from

| Candidate | Why consider it | Live transcript tradeoff | Czech |
| --- | --- | --- | --- |
| Moonshine Streaming, especially Small 123M | Small native streaming engine for CPU devices; Tiny 34M and Medium 245M are alternatives | Incremental processing; actual update latency depends on hardware and stream settings | No current Czech checkpoint |
| NVIDIA Parakeet TDT 0.6B v3 | Practical multilingual local candidate, punctuation, 25 European languages | Official buffered streaming example uses 2-second chunks and 2-second right context; not a sub-200-ms promise | Yes |
| Qwen3-ASR 0.6B | Smaller Qwen option with multilingual recognition and streaming support | Official streaming backend requires vLLM; the reported 92-ms time-to-first-token is a server inference measurement, not microphone-to-stable-text latency | Yes |
| Whisper large-v3-turbo | Mature multilingual baseline, smaller decoder than large-v3 | No native streaming; a wrapper must supply rolling/chunked previews | Yes |
| Qwen3-ASR 1.7B | Accuracy-oriented option, also supports streaming | Heavier than 0.6B; assess Czech accuracy and end-to-end latency on the target machine | Yes |

For this Linux app, benchmark Parakeet first for Czech and English. Use Moonshine for an English-first, small native-streaming mode if that becomes a priority. Qwen is promising, but adopting its official streaming stack would substantially increase packaging and hardware requirements. Voxtral Mini 4B Realtime has a native streaming architecture and a published 480-ms operating point, but its 13-language set does not include Czech and its full precision weights are too large for a compact 5-GB setup; it is not selected here.

Sources and observed quality:

- [Moonshine model catalog](https://moonshine-voice.readthedocs.io/en/stable/models/available-models/): maintained by the implementation authors; distinguishes streaming models, languages, licenses and noncomparable evaluations.
- [Parakeet v3 model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3): original model publisher; includes Czech evaluation, languages and the buffered streaming configuration.
- [Qwen3-ASR implementation](https://github.com/QwenLM/Qwen3-ASR) and [technical report](https://arxiv.org/abs/2601.21337): original authors; documents backend restrictions and streaming evaluation with two-second chunks. Performance claims need independent target-device validation.
- [Whisper Turbo model card](https://huggingface.co/openai/whisper-large-v3-turbo): original model publisher; explains the decoder reduction and accuracy tradeoff.
- [Voxtral Realtime model card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602): original model publisher; distinguishes configurable delay and model size.
- [ONNX ASR usage](https://istupakov.github.io/onnx-asr/usage/): runtime maintainer; documents local paths, quantization and supported conversions.

## Implemented size slider

The shortlist above compares model families. The initial self-contained Linux slider instead prioritizes a small common runtime, Czech coverage at every stop, and a useful size range: Whisper Tiny (42 MB), Base (78 MB), Small (250 MB), Parakeet v3 INT8 (670 MB, recommended), and Whisper Turbo INT8 (1,086 MB). Approximate decimal sizes refer to downloaded files, not RAM. All five together occupy about 2.13 GB before the runtime.

Mluva owns the files in its XDG data directory; it does not borrow another app's configuration, process or models. Revision pins, expected sizes and weight hashes are in `linux/mluva_linux/local_models.json`. Only a complete verified download unlocks Continue/Apply. Interrupted downloads leave no ready model. Downloads have a five-billion-byte aggregate model-storage guard and a free-space check. Runtime packages are additional installation storage.

The CPU worker loads on first audio, stays available during that recording, and exits at Stop/Cancel. It inherits no cloud credentials, loads only local paths with offline flags, and has a five-billion-byte address-space ceiling. This is a worker limit, not a cap on the entire GTK application plus other processes. Selecting Local never falls back to a paid speech API.

The initial local preview uses three-second chunks and keeps provisional text separate from final recognition. It is not native word-by-word streaming. Stop recognizes the recording again, in bounded 25-second blocks, so long recordings may have boundary artifacts. A rolling overlap/alignment implementation and a representative Czech/English accuracy benchmark are future work, not established capabilities of this revision.

## ElevenLabs recommendation copy

[ElevenLabs API pricing](https://elevenlabs.io/pricing/api) lists Scribe v2 Realtime at $0.39/hour excluding taxes, and advertises approximately 150-ms latency. At that usage rate, $5 corresponds to about 12.8 hours. This calculation is not a statement that a particular account offers a $5 top-up or that all plans have identical terms. The onboarding describes it as recommended and fast, without an unsupported universal “best/fastest” claim, and includes Daniel's non-affiliation statement.

## Local smoke measurements

On the development laptop (i7-12700H, CPU execution, four inference threads), all five entries transcribed the same 15.05-second public English clip from the [Qwen example](https://qianwen-res.oss-cn-beijing.aliyuncs.com/Qwen3-ASR-Repo/asr_en.wav). The clip was converted to 16-kHz mono PCM before testing. “Cold” includes launching the worker and loading weights; “warm” is a second request in the same worker. These are one-sample smoke timings under normal desktop load, not accuracy scores or live display latency.

| Managed model | Cold seconds | Warm seconds |
| --- | ---: | ---: |
| Whisper Tiny | 1.63 | 0.82 |
| Whisper Base | 1.97 | 1.27 |
| Whisper Small | 3.96 | 2.92 |
| Parakeet v3 | 3.21 | 1.04 |
| Whisper Turbo | 10.51 | 9.03 |

Parakeet is the practical default from this test. Turbo is a poor low-delay choice on this CPU. Three consecutive Turbo runs passed after disabling oversized ONNX memory arenas; the largest measured child RSS in that check was 2.20 GB. The recording lifecycle check verified unloaded startup, visible local preview, and worker exit on both Stop and Cancel. These checks do not establish Czech recognition accuracy, worst-case RAM across all recordings, or sub-second microphone-to-text latency.
