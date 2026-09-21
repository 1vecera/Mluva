"""Capability restrictions for a text-only Codex app-server child."""

import shutil
from collections.abc import Mapping
from pathlib import Path

# Apply these before initialization as well as at thread creation. Empty tables
# do not clear Codex's inherited configuration: MCP servers are disabled by name.
TEXT_ONLY_CONFIG: dict[str, object] = {
    "features.shell_tool": False,
    "features.unified_exec": False,
    "features.apply_patch_freeform": False,
    "features.view_image": False,
    "features.apps": False,
    "features.plugins": False,
    "features.hooks": False,
    "features.plugin_hooks": False,
    "features.multi_agent": False,
    "features.multi_agent_v2": False,
    "features.code_mode": False,
    "features.code_mode_host": False,
    "features.js_repl": False,
    "features.browser_use": False,
    "features.in_app_browser": False,
    "features.in_app_local_automation": False,
    "features.computer_use": False,
    "features.image_generation": False,
    "features.memories": False,
    "features.memory_tool": False,
    "features.skill_search": False,
    "features.skip_host_skill_discovery": True,
    "features.skill_mcp_dependency_install": False,
    "features.tool_suggest": False,
    "features.tool_search": False,
    "features.search_tool": False,
    "features.standalone_web_search": False,
    "features.goals": False,
    "features.token_budget": False,
    "features.sleep_tool": False,
    "features.request_permissions_tool": False,
    "orchestrator.mcp.enabled": False,
    "orchestrator.skills.enabled": False,
    "skills.include_instructions": False,
    "skills.bundled.enabled": False,
    "tools.experimental_request_user_input.enabled": False,
    "tools.update_plan.enabled": False,
    "instructions": "",
    "web_search": "disabled",
    "project_doc_max_bytes": 0,
    "notify": [],
    "developer_instructions": "",
    "shell_environment_policy.inherit": "none",
    "shell_environment_policy.set": {},
    "history.persistence": "none",
    "analytics.enabled": False,
}


class CodexIsolationError(RuntimeError):
    """Explain a missing local isolation requirement without exposing configuration."""


def child_environment(environ: Mapping[str, str]) -> dict[str, str]:
    """Retain Codex login, locale and transport settings without unrelated service credentials."""
    allowed = {
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "CODEX_HOME",
        "OPENAI_API_KEY",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    }
    return {key: value for key, value in environ.items() if key in allowed}


def mask_global_instructions(command: list[str], environ: Mapping[str, str]) -> list[str]:
    """Hide global instruction files in a child mount namespace while keeping Codex's own login store."""
    codex_home = Path(environ.get("CODEX_HOME", str(Path(environ["HOME"]) / ".codex"))).resolve()
    instruction_paths = [codex_home / name for name in ("AGENTS.md", "AGENTS.override.md")]
    existing = [path for path in instruction_paths if path.is_file()]
    if not existing:
        return command
    executable = shutil.which("bwrap")
    if executable is None:
        raise CodexIsolationError("Install bubblewrap to isolate Codex's global instructions before rewriting.")
    # This mount namespace only masks instructions; environments=[] and disabled
    # tools enforce the capability boundary. No host files are edited or copied.
    wrapper = [executable, "--die-with-parent", "--new-session", "--bind", "/", "/"]
    for path in existing:
        wrapper.extend(("--ro-bind", "/dev/null", str(path)))
    return [*wrapper, "--", *command]
