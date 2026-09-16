"""Isolated, offline ONNX speech inference; exits when its recording owner closes input."""

import json
import os
import resource
import sys
import wave
from pathlib import Path


def main() -> None:
    """Load one model on demand and reuse it only for this recording's requests."""
    import numpy as np
    import onnx_asr
    import onnxruntime as ort

    from mluva_linux.local_models import MODEL_BY_ID

    model = MODEL_BY_ID[sys.argv[1]]
    # Bound address space before creating inference sessions. The child never
    # inherits cloud keys; local paths and offline flags prohibit hub fallback.
    device = sys.argv[3] if len(sys.argv) > 3 else "cpu"
    if device == "cpu":
        resource.setrlimit(resource.RLIMIT_AS, (5_000_000_000, 5_000_000_000))
    else:
        ort.preload_dlls(directory="")
        if "CUDAExecutionProvider" not in ort.get_available_providers():
            raise RuntimeError("CUDA is unavailable")
    options = ort.SessionOptions()
    options.intra_op_num_threads = min(4, os.cpu_count() or 1)
    options.inter_op_num_threads = 1
    # Avoid arena growth reserving large slabs beyond the small-worker budget.
    options.enable_cpu_mem_arena = False
    options.enable_mem_pattern = False
    if device == "cuda" and model["engine"] == "whisper":
        # ORT 1.26's MatMulNBits rewrite breaks these merged quantized decoder graphs.
        options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_DISABLE_ALL
    engine = onnx_asr.load_model(
        model["engine"],
        Path(sys.argv[2]),
        quantization=model["quantization"],
        sess_options=options,
        providers=[
            (
                "CUDAExecutionProvider",
                {
                    "gpu_mem_limit": 2_500_000_000,
                    "cudnn_conv_use_max_workspace": "0",
                    "arena_extend_strategy": "kSameAsRequested",
                },
            ),
            "CPUExecutionProvider",
        ]
        if device == "cuda"
        else ["CPUExecutionProvider"],
    )
    if device == "cuda":
        sessions = [value for value in vars(engine.asr).values() if isinstance(value, ort.InferenceSession)]
        if not sessions or any("CUDAExecutionProvider" not in session.get_providers() for session in sessions):
            raise RuntimeError("GPU initialization failed. Choose CPU or reinstall GPU support.")
        for session in sessions:
            session.disable_fallback()
    for line in sys.stdin:
        request = json.loads(line)
        parts = []
        with wave.open(request["path"], "rb") as source:
            if (source.getnchannels(), source.getsampwidth(), source.getframerate()) != (1, 2, 16000):
                raise ValueError("Expected 16 kHz mono PCM")
            while frames := source.readframes(25 * 16000):
                audio = np.frombuffer(frames, dtype="<i2").astype(np.float32) / 32768
                result = engine.recognize(
                    audio,
                    sample_rate=16000,
                    **({"language": request["language"]} if request["language"] != "auto" else {}),
                )
                parts.append(result)
        print(json.dumps({"text": " ".join(parts).strip()}), flush=True)


if __name__ == "__main__":
    main()
