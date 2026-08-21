"""Typed synchronous client for the Codex app-server JSONL transport."""

import json
import queue
import subprocess
import threading
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Self

from voice_scribe_linux.brand import PRODUCT_NAME, PRODUCT_VERSION

_SERVER_EXITED_METHOD = "_voice_scribe/serverExited"
MODEL_PAGE_SIZE = 100
MAX_MODEL_PAGES = 10
MAX_TRANSFORMATION_OUTPUT_CHARACTERS = 8_000


class CodexAppServerError(RuntimeError):
    """Report protocol, lifecycle, and server-side Codex failures."""


@dataclass(slots=True)
class CodexAppServerClient:
    """Run bounded text transformations through the installed Codex app-server."""

    command: Sequence[str] = ("codex", "app-server", "--listen", "stdio://")
    request_timeout_seconds: float = 30
    turn_timeout_seconds: float = 180
    process: subprocess.Popen[str] | None = None
    last_model_identifier: str | None = field(init=False, default=None)
    _next_request_id: int = 0
    _responses: dict[int, queue.Queue[dict[str, object]]] = field(default_factory=dict)
    _notifications: queue.Queue[dict[str, object]] = field(default_factory=queue.Queue)
    _lock: threading.Lock = field(default_factory=threading.Lock)
    _cancel_requested: threading.Event = field(default_factory=threading.Event)
    _reader: threading.Thread | None = None

    def start(self) -> None:
        """Start and initialize one app-server transport connection."""
        if self._cancel_requested.is_set():
            raise CodexAppServerError("Codex app-server work was cancelled.")
        if self.process is not None:
            if self.process.poll() is None and self._reader is not None and self._reader.is_alive():
                return
            self.close()
        try:
            self.process = subprocess.Popen(
                list(self.command),
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                bufsize=1,
            )
        except OSError as error:
            raise CodexAppServerError("Codex app-server could not start.") from error
        if self._cancel_requested.is_set():
            self.close()
            raise CodexAppServerError("Codex app-server work was cancelled.")
        self._reader = threading.Thread(target=self._read_messages, name="codex-app-server-reader", daemon=True)
        self._reader.start()
        try:
            self._request(
                "initialize",
                {
                    "clientInfo": {
                        "name": "mluva-linux",
                        "title": PRODUCT_NAME,
                        "version": PRODUCT_VERSION,
                    },
                    "capabilities": {"experimentalApi": False},
                },
            )
            self._send({"method": "initialized", "params": {}})
        except Exception:
            self.close()
            raise

    def close(self) -> None:
        """Terminate the child server without leaving a background process."""
        process = self.process
        if process is None:
            return
        self.process = None
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        if process.stdin is not None:
            process.stdin.close()
        reader = self._reader
        if reader is not None and reader is not threading.current_thread():
            reader.join(timeout=5)
        if process.stdout is not None:
            process.stdout.close()
        self._reader = None
        while True:
            try:
                self._notifications.get_nowait()
            except queue.Empty:
                break

    def cancel(self) -> None:
        """Prevent future startup and interrupt any active request or turn."""
        self._cancel_requested.set()
        self.close()

    def spawn(self) -> Self:
        """Return an unstarted client with identical transport bounds for isolated concurrent work."""
        return type(self)(
            command=tuple(self.command),
            request_timeout_seconds=self.request_timeout_seconds,
            turn_timeout_seconds=self.turn_timeout_seconds,
        )

    def transform(self, prompt: str, cwd: Path, model: str | None = None) -> str:
        """Return only the final agent text for one isolated transformation."""
        resolved_model = model or self.resolve_model(None)
        self.start()
        self.last_model_identifier = None
        thread_params: dict[str, object] = {
            "cwd": str(cwd),
            "model": resolved_model,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "ephemeral": True,
            "serviceName": "voice_scribe_linux",
            "baseInstructions": (
                "You transform dictated text. Follow the user's requested operation exactly. "
                "Return only replacement text, without commentary, quotes, or Markdown fences. "
                "Never use tools or infer facts absent from the supplied text."
            ),
        }
        thread_result = self._request("thread/start", thread_params)
        actual_model = thread_result["model"]
        if actual_model != resolved_model:
            raise CodexAppServerError("Codex app-server changed the frozen model for this transformation.")
        self.last_model_identifier = actual_model
        thread = thread_result["thread"]
        thread_id = thread["id"]
        started = self._request(
            "turn/start",
            {"threadId": thread_id, "input": [{"type": "text", "text": prompt}]},
        )
        turn_id = started["turn"]["id"]
        output: list[str] = []
        output_characters = 0
        while True:
            try:
                message = self._notifications.get(timeout=self.turn_timeout_seconds)
            except queue.Empty as error:
                raise CodexAppServerError("Codex app-server timed out while producing text.") from error
            if message.get("method") == _SERVER_EXITED_METHOD:
                raise CodexAppServerError("Codex app-server exited before completing the text transformation.")
            params = message["params"]
            if params.get("threadId") != thread_id:
                continue
            if message["method"] == "item/agentMessage/delta" and params["turnId"] == turn_id:
                delta = params["delta"]
                if not isinstance(delta, str):
                    self.close()
                    raise CodexAppServerError("Codex returned malformed replacement text.")
                output_characters += len(delta)
                if output_characters > MAX_TRANSFORMATION_OUTPUT_CHARACTERS:
                    self.close()
                    raise CodexAppServerError("Codex replacement text exceeded the supported bound.")
                output.append(delta)
            if message["method"] == "turn/completed" and params["turn"]["id"] == turn_id:
                status = params["turn"]["status"]
                if status != "completed":
                    raise CodexAppServerError(f"Codex turn ended with status {status}.")
                result = "".join(output).strip()
                if not result:
                    raise CodexAppServerError("Codex returned no replacement text.")
                return result

    def resolve_model(self, requested_model: str | None) -> str:
        """Resolve one configured or default app-server model to the concrete identifier used by a capture."""
        self.start()
        cursor: str | None = None
        default_models: list[str] = []
        for _page in range(MAX_MODEL_PAGES):
            params: dict[str, object] = {"includeHidden": True, "limit": MODEL_PAGE_SIZE}
            if cursor is not None:
                params["cursor"] = cursor
            result = self._request("model/list", params)
            for model in result["data"]:
                identifier = model["model"]
                if requested_model is not None and requested_model in {model["id"], identifier}:
                    return identifier
                if model["isDefault"]:
                    default_models.append(identifier)
            cursor = result["nextCursor"] if "nextCursor" in result else None
            if cursor is None:
                break
        else:
            raise CodexAppServerError("Codex app-server returned too many model-list pages.")
        if requested_model is not None:
            raise CodexAppServerError("The configured Codex model is unavailable.")
        if len(default_models) != 1:
            raise CodexAppServerError("Codex app-server did not expose exactly one default model.")
        return default_models[0]

    def _request(self, method: str, params: dict[str, object]) -> dict[str, object]:
        """Send one request and correlate its response across the reader thread."""
        with self._lock:
            request_id = self._next_request_id
            self._next_request_id += 1
            response_queue: queue.Queue[dict[str, object]] = queue.Queue(maxsize=1)
            self._responses[request_id] = response_queue
            try:
                self._send({"method": method, "id": request_id, "params": params})
            except (OSError, ValueError) as error:
                self._responses.pop(request_id, None)
                raise CodexAppServerError(f"Codex app-server connection failed during {method}.") from error
        try:
            response = response_queue.get(timeout=self.request_timeout_seconds)
        except queue.Empty as error:
            raise CodexAppServerError(f"Codex app-server did not answer {method}.") from error
        finally:
            with self._lock:
                self._responses.pop(request_id, None)
        if "error" in response:
            raise CodexAppServerError(f"Codex app-server rejected {method}.")
        return response["result"]

    def _send(self, message: dict[str, object]) -> None:
        """Write one complete JSONL protocol frame to the active process."""
        if self.process is None or self.process.stdin is None:
            raise CodexAppServerError("Codex app-server is not running.")
        self.process.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
        self.process.stdin.flush()

    def _read_messages(self) -> None:
        """Dispatch response and notification frames without blocking callers."""
        process = self.process
        if process is None or process.stdout is None:
            raise CodexAppServerError("Codex app-server stdout is unavailable.")
        try:
            for line in process.stdout:
                message = json.loads(line)
                if not isinstance(message, dict):
                    break
                if "id" in message:
                    with self._lock:
                        response_queue = self._responses.get(message["id"])
                    if response_queue is not None:
                        response_queue.put(message)
                elif "method" in message:
                    self._notifications.put(message)
        except (OSError, TypeError, ValueError):
            pass
        finally:
            failure_response: dict[str, object] = {"error": {}}
            with self._lock:
                response_queues = tuple(self._responses.values())
            for response_queue in response_queues:
                try:
                    response_queue.put_nowait(failure_response)
                except queue.Full:
                    pass
            self._notifications.put({"method": _SERVER_EXITED_METHOD, "params": {}})
