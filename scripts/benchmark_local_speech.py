"""Feed a WAV at microphone speed through the real local preview/finalization path.

Run with PYTHONPATH=linux uv run --project linux --locked python
scripts/benchmark_local_speech.py MODEL --device cpu --audio sample.wav --output tmp/benchmark.json.
Use a task-specific XDG_DATA_HOME containing models downloaded by Mluva first.
This command never downloads models or captures a microphone.
"""

import argparse
import json
import subprocess
import threading
import time
import wave
from pathlib import Path

from mluva_linux.local_models import MODEL_BY_ID
from mluva_linux.local_preview import LocalPreviewClient


def benchmark(model, device, audio, output):
    """Measure cold process startup, provisional updates, Stop, memory and worker cleanup."""
    with wave.open(str(audio), "rb") as source:
        if (source.getnchannels(), source.getsampwidth(), source.getframerate()) != (
            1,
            2,
            16000,
        ):
            raise ValueError("Use 16 kHz mono 16-bit PCM")
        pcm = source.readframes(source.getnframes()) * 2
    output.parent.mkdir(parents=True, exist_ok=True)
    session = LocalPreviewClient(model, output.parent / "audio", device=device).start(
        "eng"
    )
    result = {
        "model": model,
        "device": device,
        "audio_seconds": len(pcm) / 32000,
        "unloaded_before_audio": session.local_client is None,
        "native_acoustic_streaming": False,
        "updates": [],
    }
    done = threading.Event()
    pids, gpu_memory = set(), []
    peak_rss = [0]

    def monitor():
        tick = 0
        while not done.wait(0.1):
            tick += 1
            if device == "cuda" and tick % 10 == 0:
                try:
                    used = subprocess.check_output(
                        [
                            "nvidia-smi",
                            "--query-gpu=memory.used",
                            "--format=csv,noheader,nounits",
                        ],
                        timeout=2,
                        text=True,
                    )
                    gpu_memory.append(int(used.strip()))
                except (OSError, ValueError, subprocess.SubprocessError):
                    pass
            client = session.local_client
            process = client.process if client else None
            if process:
                pids.add(process.pid)
                try:
                    lines = Path(f"/proc/{process.pid}/status").read_text().splitlines()
                    rss = next(
                        int(line.split()[1]) * 1024
                        for line in lines
                        if line.startswith("VmRSS:")
                    )
                    peak_rss[0] = max(peak_rss[0], rss)
                except (OSError, StopIteration):
                    pass

    observer = threading.Thread(target=monitor, daemon=True)
    observer.start()
    started, previous = time.monotonic(), ""
    try:
        for offset in range(0, len(pcm), 1600):
            time.sleep(max(0, started + offset / 32000 - time.monotonic()))
            session.submit_audio(pcm[offset : offset + 1600])
            text = session.snapshot().display_text
            if text and text != previous:
                result["updates"].append(
                    {"seconds": round(time.monotonic() - started, 3), "text": text}
                )
                previous = text
        result["healthy_before_stop"] = session.is_healthy
        stopped = time.monotonic()
        final = session.finish()
        result["stop_seconds"] = round(time.monotonic() - stopped, 3)
        result["text"] = final.transcription.text
    except (RuntimeError, OSError, ValueError) as error:
        result["error"] = str(error)
    finally:
        session.cancel()
        done.set()
        observer.join(timeout=3)
        time.sleep(0.2)
    result.update(
        peak_rss_bytes=peak_rss[0],
        peak_total_nvidia_memory_mib=max(gpu_memory) if gpu_memory else None,
        workers_released=all(not Path(f"/proc/{pid}").exists() for pid in pids),
        first_readable_seconds=result["updates"][0]["seconds"]
        if result["updates"]
        else None,
        update_count=len(result["updates"]),
    )
    result["live_verified"] = bool(
        result["updates"]
        and result.get("healthy_before_stop")
        and result.get("text")
        and result["workers_released"]
    )
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(
        json.dumps(
            {
                key: value
                for key, value in result.items()
                if key not in ("updates", "text")
            }
        )
    )
    return result["live_verified"]


def main():
    """Keep download approval and model-store selection outside the benchmark."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", choices=MODEL_BY_ID)
    parser.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    parser.add_argument("--audio", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if not benchmark(
        arguments.model, arguments.device, arguments.audio, arguments.output
    ):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
