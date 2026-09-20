"""On-demand Qwen recognition through an owned, authenticated loopback runtime."""

import base64
import io
import json
import os
import re
import secrets
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
import wave
from pathlib import Path

from mluva_linux import local_qwen
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.local_models import model_path, ready
from mluva_linux.speech_languages import ISO_CODES, QWEN_LANGUAGES

_LANGUAGE_NAMES = dict(
    zip(
        (
            "en",
            "zh",
            "yue",
            "ar",
            "de",
            "fr",
            "es",
            "pt",
            "id",
            "it",
            "ko",
            "ru",
            "th",
            "vi",
            "ja",
            "tr",
            "hi",
            "ms",
            "nl",
            "sv",
            "da",
            "fi",
            "pl",
            "cs",
            "fil",
            "fa",
            "el",
            "hu",
            "mk",
            "ro",
        ),
        (
            "English",
            "Chinese",
            "Cantonese",
            "Arabic",
            "German",
            "French",
            "Spanish",
            "Portuguese",
            "Indonesian",
            "Italian",
            "Korean",
            "Russian",
            "Thai",
            "Vietnamese",
            "Japanese",
            "Turkish",
            "Hindi",
            "Malay",
            "Dutch",
            "Swedish",
            "Danish",
            "Finnish",
            "Polish",
            "Czech",
            "Filipino",
            "Persian",
            "Greek",
            "Hungarian",
            "Macedonian",
            "Romanian",
        ),
        strict=True,
    )
)


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args):
        return None


class QwenSpeechClient:
    """Keep one local process per recording and stream provisional decoder text."""

    def __init__(self, model="qwen3-1.7b", *, keep_alive=False, device="cpu"):
        """Store configuration only; create no process or model allocation yet."""
        self.model, self.keep_alive, self.device = model, keep_alive, device
        self.process = None
        self.lifecycle_lock = threading.RLock()
        self.cancelled = threading.Event()
        self.monitor_stop = threading.Event()
        self.monitor = None
        self.temporary = None
        self.token = ""
        self.url = ""
        self.failure = ""
        self.on_partial = None
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())

    def _start(self):
        if not ready(self.model) or not local_qwen.ready(self.device):
            raise RuntimeError("Download Qwen and its runtime in Settings → Providers first.")
        if self.cancelled.is_set():
            raise RuntimeError("Local transcription cancelled.")
        environment = {key: os.environ[key] for key in ("PATH", "LANG", "HOME") if key in os.environ}
        cache = local_qwen.runtime_root().parent / "qwen-cache"
        cache.mkdir(mode=0o700, parents=True, exist_ok=True)
        environment.update(
            HF_HUB_OFFLINE="1", XDG_CACHE_HOME=str(cache), __GL_SHADER_DISK_CACHE_PATH=str(cache), OMP_NUM_THREADS="4"
        )
        executable = local_qwen.binary(self.device)
        target = "none"
        if self.device == "cuda":
            result = subprocess.run(
                [str(executable), "--list-devices"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=15,
                check=True,
            )
            devices = re.findall(r"^\s*(Vulkan\d+): (.+)$", result.stdout, flags=re.MULTILINE)
            target = next((key for key, name in devices if "NVIDIA" in name), "")
            if not target:
                raise RuntimeError("No supported NVIDIA Vulkan device found. Choose CPU.")
        with self.lifecycle_lock:
            if self.cancelled.is_set():
                raise RuntimeError("Local transcription cancelled.")
            self.temporary = tempfile.TemporaryDirectory(prefix="mluva-qwen-")
            self.token = secrets.token_urlsafe(32)
            key_path = Path(self.temporary.name) / "key"
            key_path.write_text(self.token)
            key_path.chmod(0o600)
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                port = listener.getsockname()[1]
            self.url = f"http://127.0.0.1:{port}"
            path = model_path(self.model)
            arguments = [
                str(executable),
                "-m",
                str(path / "Qwen3-ASR-1.7B-Q4_0.gguf"),
                "--mmproj",
                str(path / "mmproj-Qwen3-ASR-1.7B-Q8_0.gguf"),
                "--host",
                "127.0.0.1",
                "--port",
                str(port),
                "--api-key-file",
                str(key_path),
                "--no-webui",
                "--log-disable",
                "-c",
                "2048",
                "-np",
                "1",
                "-t",
                "4",
                "-tb",
                "4",
                "--fit",
                "off",
                "--device",
                target,
                "-ngl",
                "0" if target == "none" else "99",
            ]
            if target == "none":
                arguments.extend(("--no-mmproj-offload", "--no-op-offload"))
            self.process = subprocess.Popen(
                arguments,
                env=environment,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            self.monitor_stop.clear()
            self.monitor = threading.Thread(
                target=self._watch_memory, args=(self.process,), daemon=True, name="qwen-memory"
            )
            self.monitor.start()
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if self.cancelled.is_set() or self.process is None or self.process.poll() is not None:
                raise RuntimeError(self.failure or "Qwen could not start. Try CPU or download the runtime again.")
            try:
                with self.opener.open(self.url + "/health", timeout=0.5) as response:
                    if response.status == 200:
                        return
            except (OSError, urllib.error.URLError):
                self.cancelled.wait(0.1)
        raise RuntimeError("Qwen startup timed out. Try a smaller model.")

    def _watch_memory(self, process):
        while not self.monitor_stop.wait(0.1) and process.poll() is None:
            try:
                status = Path(f"/proc/{process.pid}/status").read_text()
                rss = next(int(line.split()[1]) * 1024 for line in status.splitlines() if line.startswith("VmRSS:"))
                if rss > 5_000_000_000:
                    self.failure = "Local model exceeded 5 GB RAM. Choose a smaller model."
                    process.kill()
                    return
            except (OSError, StopIteration):
                return

    def transcribe(self, file_path, language_code, model_id=""):
        """Recognize bounded audio blocks, exposing only text after the ASR prefix."""
        language = ISO_CODES.get(language_code, language_code)
        if language != "auto" and language not in {ISO_CODES[code] for code in QWEN_LANGUAGES}:
            raise RuntimeError("This language is not supported by Qwen. Choose another local model.")
        try:
            if self.process is None:
                self._start()
            parts = []
            with wave.open(str(file_path), "rb") as source:
                if (source.getnchannels(), source.getsampwidth(), source.getframerate()) != (1, 2, 16000):
                    raise ValueError("Expected 16 kHz mono PCM")
                while frames := source.readframes(25 * 16000):
                    buffer = io.BytesIO()
                    with wave.open(buffer, "wb") as writer:
                        writer.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                        writer.writeframes(frames)
                    parts.append(self._recognize(buffer.getvalue(), language, " ".join(parts)))
            return TranscriptionResult(" ".join(parts).strip(), language_code, None, None)
        except RuntimeError:
            self.close()
            raise
        except (OSError, ValueError, TypeError, AttributeError, subprocess.SubprocessError):
            self.close()
            raise RuntimeError(self.failure or "Qwen transcription failed. Try CPU or a smaller model.") from None
        finally:
            if not self.keep_alive:
                self.close()

    def _recognize(self, audio, language, previous):
        messages = [
            {
                "role": "user",
                "content": [
                    {"type": "input_audio", "input_audio": {"data": base64.b64encode(audio).decode(), "format": "wav"}}
                ],
            }
        ]
        body = {"messages": messages, "temperature": 0, "max_tokens": 512, "stream": True, "cache_prompt": False}
        if language != "auto":
            messages.append({"role": "assistant", "content": f"language {_LANGUAGE_NAMES[language]}<asr_text>"})
            body.update(continue_final_message=True, add_generation_prompt=False)
        request = urllib.request.Request(
            self.url + "/v1/chat/completions",
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer " + self.token},
        )
        text, transcript = "", ""
        completed = False
        deadline = time.monotonic() + 180
        with self.opener.open(request, timeout=180) as response:
            while line := response.readline(65537):
                if self.cancelled.is_set():
                    raise RuntimeError("Local transcription cancelled.")
                if time.monotonic() > deadline or len(line) > 65536:
                    raise RuntimeError("Local transcription timed out or returned too much data.")
                if line.strip() == b"data: [DONE]":
                    completed = True
                    break
                if not line.startswith(b"data: "):
                    continue
                event = json.loads(line[6:])
                choices = event.get("choices") or []
                if not choices:
                    continue
                if choices[0].get("finish_reason") == "length":
                    raise RuntimeError("Local transcript exceeded its output limit. Try a shorter recording.")
                content = choices[0].get("delta", {}).get("content")
                if content is None:
                    continue
                if not isinstance(content, str):
                    raise ValueError("Malformed local transcript")
                text += content
                if len(text) > 100_000:
                    raise RuntimeError("Local transcription returned too much data.")
                transcript = (
                    text.split("<asr_text>", 1)[-1] if "<asr_text>" in text else text if language != "auto" else ""
                )
                if self.on_partial and transcript:
                    self.on_partial((previous + " " + transcript).strip())
        if not completed:
            raise RuntimeError("Local transcription stream ended early. Please retry the recording.")
        return transcript.strip()

    def close(self):
        """Stop only this recording's process and remove its temporary credential."""
        with self.lifecycle_lock:
            self._close()

    def _close(self):
        self.monitor_stop.set()
        process, self.process = self.process, None
        if process is not None:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
            process.wait()
        if self.monitor and self.monitor is not threading.current_thread():
            self.monitor.join(timeout=1)
        if self.temporary:
            self.temporary.cleanup()
            self.temporary = None
        self.token = ""

    def cancel(self):
        """Invalidate all late text and stop inference immediately."""
        self.cancelled.set()
        self.close()
