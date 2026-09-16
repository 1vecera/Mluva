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
    resource.setrlimit(resource.RLIMIT_AS, (5_000_000_000, 5_000_000_000))
    options = ort.SessionOptions()
    options.intra_op_num_threads = min(4, os.cpu_count() or 1)
    options.inter_op_num_threads = 1
    # Avoid arena growth reserving large slabs beyond the small-worker budget.
    options.enable_cpu_mem_arena = False
    options.enable_mem_pattern = False
    engine = onnx_asr.load_model(
        model["engine"],
        Path(sys.argv[2]),
        quantization=model["quantization"],
        sess_options=options,
        providers=["CPUExecutionProvider"],
    )
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
