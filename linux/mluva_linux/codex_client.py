"""Typed synchronous client for the Codex app-server JSONL transport."""

import json
import os
import queue
import shutil
import subprocess
import tempfile
import threading
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Self

from mluva_linux.brand import PRODUCT_NAME, PRODUCT_VERSION
from mluva_linux.codex_policy import (
    TEXT_ONLY_CONFIG,
    CodexIsolationError,
    child_environment,
    mask_global_instructions,
)

_SERVER_EXITED_METHOD = "_mluva/serverExited"
_UNEXPECTED_REQUEST_METHOD = "_mluva/unexpectedRequest"
MODEL_PAGE_SIZE = 100
MAX_MODEL_PAGES = 10
MAX_TRANSFORMATION_OUTPUT_CHARACTERS = 8_000


class CodexAppServerError(RuntimeError):
    """Report protocol, lifecycle, and server-side Codex failures."""


@dataclass(frozen=True, slots=True)
class CodexModel:
    """Keep model choices and rewrite speed settings tied to the installed server's catalog."""

    id: str
    identifier: str
    name: str
    is_default: bool
    hidden: bool = False
    rewrite_effort: str | None = None
    fast_tier: str | None = None
    reasoning_efforts: tuple[str, ...] = ()

    @classmethod
    def from_catalog(cls, model: dict[str, object]) -> Self:
        """Accept older catalogs without inventing unsupported effort or service tiers."""
        efforts = {option["reasoningEffort"] for option in model.get("supportedReasoningEfforts", [])}
        fast_tier = next(
            (
                tier["id"]
                for tier in model.get("serviceTiers", [])
                if tier["name"].casefold() == "fast" or tier["id"] in {"fast", "priority"}
            ),
            None,
        )
        if fast_tier is None and "fast" in model.get("additionalSpeedTiers", []):
            fast_tier = "fast"
        return cls(
            id=model["id"],
            identifier=model["model"],
            name=model.get("displayName", model["model"]),
            is_default=model["isDefault"],
            hidden=model.get("hidden", False),
            rewrite_effort="low" if "low" in efforts else model.get("defaultReasoningEffort"),
            fast_tier=fast_tier,
            reasoning_efforts=tuple(option["reasoningEffort"] for option in model.get("supportedReasoningEfforts", [])),
        )


def select_model(models: Sequence[CodexModel], requested_model: str | None) -> CodexModel:
    """Resolve an explicit identifier or exactly one advertised default without silently switching models."""
    if requested_model is not None:
        for model in models:
            if requested_model in {model.id, model.identifier}:
                return model
        raise CodexAppServerError("The configured Codex model is unavailable.")
    defaults = [model for model in models if model.is_default]
    if len(defaults) != 1:
        raise CodexAppServerError("Codex app-server did not expose exactly one default model.")
    return defaults[0]


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
    _workspace: tempfile.TemporaryDirectory | None = field(default=None, init=False, repr=False)

    def start(self) -> None:
        """Start and initialize one app-server transport connection."""
        if self._cancel_requested.is_set():
            raise CodexAppServerError("Codex app-server work was cancelled.")
        if self.process is not None:
            if self.process.poll() is None and self._reader is not None and self._reader.is_alive():
                return
            self.close()
        try:
            workspace = tempfile.TemporaryDirectory(prefix="mluva-codex-")
            self._workspace = workspace
            command = list(self.command)
            executable = shutil.which(command[0])
            if executable is None:
                raise OSError("Codex executable is unavailable.")
            command[0] = str(Path(executable).absolute())
            command.append("--strict-config")
            for key, value in TEXT_ONLY_CONFIG.items():
                command.extend(("-c", f"{key}={json.dumps(value)}"))
            environment = child_environment(os.environ)
            command = mask_global_instructions(command, environment)
            self.process = subprocess.Popen(
                command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                bufsize=1,
                cwd=workspace.name,
                env=environment,
            )
        except CodexIsolationError as error:
            self.close()
            raise CodexAppServerError(str(error)) from error
        except OSError as error:
            self.close()
            raise CodexAppServerError("Codex app-server could not start.") from error
        if self._cancel_requested.is_set():
            self.close()
            raise CodexAppServerError("Codex app-server work was cancelled.")
        self._reader = threading.Thread(
            target=self._read_messages, args=(self.process,), name="codex-app-server-reader", daemon=True
        )
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
                    "capabilities": {"experimentalApi": True},
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
            if self._workspace is not None:
                self._workspace.cleanup()
                self._workspace = None
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
        if self._workspace is not None:
            self._workspace.cleanup()
            self._workspace = None
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

    def transform(
        self,
        prompt: str,
        cwd: Path,
        model: str | None = None,
        *,
        max_output_characters: int = MAX_TRANSFORMATION_OUTPUT_CHARACTERS,
        on_delta: Callable[[str], None] | None = None,
        effort: str | None = None,
        service_tier: str | None = None,
    ) -> str:
        """Stream optional validated text deltas and return the complete successful transformation."""
        resolved_model = model or self.resolve_model(None)
        self.start()
        self.last_model_identifier = None
        # Resolve inherited MCP names without starting a model turn. Passing an
        # empty MCP table would merge with, rather than remove, user servers.
        workspace = self._workspace.name
        configuration = self._request("config/read", {"includeLayers": False, "cwd": workspace})["config"]
        servers = configuration.get("mcp_servers", {})
        if not isinstance(servers, dict):
            self.close()
            raise CodexAppServerError("Codex could not establish text-only permissions.")
        overrides = dict(TEXT_ONLY_CONFIG)
        overrides["mcp_servers"] = {name: {"enabled": False} for name in servers}
        thread_params: dict[str, object] = {
            "cwd": workspace,
            "model": resolved_model,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "ephemeral": True,
            "serviceName": "mluva_linux",
            "environments": [],
            "dynamicTools": [],
            "config": overrides,
            "developerInstructions": "",
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
        if (
            thread.get("environments") != []
            or thread.get("ephemeral") is not True
            or thread_result.get("instructionSources")
        ):
            self.close()
            raise CodexAppServerError("Codex cannot confirm text-only isolation. Update Codex before rewriting.")
        thread_id = thread["id"]
        inventory = self._request("mcpServerStatus/list", {"threadId": thread_id, "limit": 100})
        if inventory.get("nextCursor") or any(
            server.get("tools") or server.get("resources") or server.get("resourceTemplates")
            for server in inventory["data"]
        ):
            self.close()
            raise CodexAppServerError("Codex exposed external capabilities during text-only setup.")
        turn_params: dict[str, object] = {"threadId": thread_id, "input": [{"type": "text", "text": prompt}]}
        if effort is not None:
            turn_params["effort"] = effort
        if service_tier is not None:
            turn_params["serviceTier"] = service_tier
        started = self._request("turn/start", turn_params)
        turn_id = started["turn"]["id"]
        output: list[str] = []
        output_characters = 0
        deadline = time.monotonic() + self.turn_timeout_seconds
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise CodexAppServerError("Codex app-server timed out while producing text.")
            try:
                message = self._notifications.get(timeout=remaining)
            except queue.Empty as error:
                raise CodexAppServerError("Codex app-server timed out while producing text.") from error
            if message.get("method") == _SERVER_EXITED_METHOD:
                raise CodexAppServerError("Codex app-server exited before completing the text transformation.")
            if message.get("method") == _UNEXPECTED_REQUEST_METHOD:
                self.close()
                raise CodexAppServerError("Codex requested an operation outside text-only rewriting.")
            params = message["params"]
            if params.get("threadId") != thread_id:
                continue
            method = message["method"]
            permitted_items = {"userMessage", "agentMessage", "reasoning"}
            items = [params.get("item", {})] if method in {"item/started", "item/completed"} else []
            if method == "turn/completed":
                items.extend(params.get("turn", {}).get("items", []))
            if any(item.get("type") not in permitted_items for item in items) or (
                method.startswith("item/")
                and method not in {"item/started", "item/completed"}
                and not method.startswith(("item/agentMessage/", "item/reasoning/"))
            ):
                self.close()
                raise CodexAppServerError("Codex attempted an operation outside text-only rewriting.")
            if message["method"] == "item/agentMessage/delta" and params["turnId"] == turn_id:
                delta = params["delta"]
                if not isinstance(delta, str):
                    self.close()
                    raise CodexAppServerError("Codex returned malformed replacement text.")
                output_characters += len(delta)
                if output_characters > max_output_characters:
                    self.close()
                    raise CodexAppServerError("Codex replacement text exceeded the supported bound.")
                output.append(delta)
                if on_delta is not None:
                    on_delta(delta)
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
        return select_model(self.list_models(), requested_model).identifier

    def list_models(self) -> list[CodexModel]:
        """Read the bounded catalog, including hidden models for existing explicit configurations."""
        self.start()
        cursor: str | None = None
        models: list[CodexModel] = []
        for _page in range(MAX_MODEL_PAGES):
            params: dict[str, object] = {"includeHidden": True, "limit": MODEL_PAGE_SIZE}
            if cursor is not None:
                params["cursor"] = cursor
            result = self._request("model/list", params)
            models.extend(CodexModel.from_catalog(model) for model in result["data"])
            cursor = result["nextCursor"] if "nextCursor" in result else None
            if cursor is None:
                break
        else:
            raise CodexAppServerError("Codex app-server returned too many model-list pages.")
        return models

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

    def _read_messages(self, process: subprocess.Popen[str]) -> None:
        """Dispatch response and notification frames without blocking callers."""
        if process.stdout is None:
            raise CodexAppServerError("Codex app-server stdout is unavailable.")
        try:
            for line in process.stdout:
                message = json.loads(line)
                if not isinstance(message, dict):
                    break
                if "id" in message and "method" in message:
                    with self._lock:
                        self._send({"id": message["id"], "error": {"code": -32601, "message": "Unsupported operation"}})
                    self._notifications.put({"method": _UNEXPECTED_REQUEST_METHOD, "params": {}})
                elif "id" in message:
                    with self._lock:
                        response_queue = self._responses.get(message["id"])
                    if response_queue is not None:
                        try:
                            response_queue.put_nowait(message)
                        except queue.Full:
                            break
                elif "method" in message:
                    self._notifications.put(message)
        except (CodexAppServerError, OSError, TypeError, ValueError):
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
