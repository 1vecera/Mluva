"""Minimal subprocess implementing the app-server frames used by contract tests."""

import json
import sys
import time
from pathlib import Path


def send(message: dict[str, object]) -> None:
    """Write one test protocol message as JSONL."""
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def main() -> None:
    """Answer initialization, thread creation, and one text transformation turn."""
    if "--fill-stderr" in sys.argv:
        sys.stderr.write("diagnostic-noise\n" * 32_768)
        sys.stderr.flush()
    for line in sys.stdin:
        message = json.loads(line)
        method = message["method"]
        if method == "initialized":
            continue
        if method == "initialize":
            send(
                {
                    "id": message["id"],
                    "result": {
                        "userAgent": "fake-codex/1.0",
                        "serverInfo": {"name": "fake-codex", "version": "1.0"},
                    },
                }
            )
        elif method == "model/list":
            assert message["params"]["includeHidden"] is True
            assert message["params"]["limit"] == 100
            send(
                {
                    "id": message["id"],
                    "result": {
                        "data": [
                            {
                                "id": "codex-default",
                                "model": "gpt-5.4",
                                "isDefault": True,
                            },
                            {
                                "id": "codex-explicit",
                                "model": "gpt-5.4-mini",
                                "isDefault": False,
                            },
                        ],
                        "nextCursor": None,
                    },
                }
            )
        elif method == "thread/start":
            assert message["params"]["sandbox"] == "read-only"
            assert message["params"]["approvalPolicy"] == "never"
            assert message["params"]["model"] in {"gpt-5.4", "gpt-5.4-mini"}
            send(
                {
                    "id": message["id"],
                    "result": {
                        "thread": {"id": "thread-test"},
                        "model": message["params"]["model"],
                    },
                }
            )
        elif method == "turn/start":
            assert message["params"]["input"][0]["type"] == "text"
            send({"id": message["id"], "result": {"turn": {"id": "turn-test"}}})
            if "--exit-during-turn" in sys.argv:
                return
            if "--title" in sys.argv:
                context = json.loads(message["params"]["input"][0]["text"].split("\n", 1)[1])
                assert 0 < len(context["transcript_excerpt"]) <= 6000
                time.sleep(0.4)
                send(
                    {
                        "method": "item/agentMessage/delta",
                        "params": {
                            "threadId": "thread-test",
                            "turnId": "turn-test",
                            "itemId": "item-test",
                            "delta": "Plán pátečního vydání",
                        },
                    }
                )
                send(
                    {
                        "method": "turn/completed",
                        "params": {
                            "threadId": "thread-test",
                            "turn": {"id": "turn-test", "status": "completed"},
                        },
                    }
                )
                continue
            if "--conversation" in sys.argv:
                context = json.loads(message["params"]["input"][0]["text"].split("\n", 1)[1])
                previous = context["completed_rewrites"]
                text = previous[-1]["text"] if previous else context["initial_text"]
                time.sleep(0.2)
                replacement = text + "\n" + context["next_instruction"]
                midpoint = len(replacement) // 2
                for delta in (replacement[:midpoint], replacement[midpoint:]):
                    send(
                        {
                            "method": "item/agentMessage/delta",
                            "params": {
                                "threadId": "thread-test",
                                "turnId": "turn-test",
                                "itemId": "item-test",
                                "delta": delta,
                            },
                        }
                    )
                    if "--completion-gate" in sys.argv:
                        gate = Path(sys.argv[sys.argv.index("--completion-gate") + 1])
                        deadline = time.monotonic() + 15
                        while not gate.exists() and time.monotonic() < deadline:
                            time.sleep(0.01)
                        assert gate.exists(), "Fixture completion was never released"
                    time.sleep(0.15)
                send(
                    {
                        "method": "turn/completed",
                        "params": {
                            "threadId": "thread-test",
                            "turn": {"id": "turn-test", "status": "completed"},
                        },
                    }
                )
                continue
            if "--oversized-output" in sys.argv:
                send(
                    {
                        "method": "item/agentMessage/delta",
                        "params": {
                            "threadId": "thread-test",
                            "turnId": "turn-test",
                            "itemId": "item-test",
                            "delta": "x" * 8_001,
                        },
                    }
                )
                continue
            send(
                {
                    "method": "item/agentMessage/delta",
                    "params": {
                        "threadId": "thread-test",
                        "turnId": "turn-test",
                        "itemId": "item-test",
                        "delta": "Clean ",
                    },
                }
            )
            send(
                {
                    "method": "item/agentMessage/delta",
                    "params": {
                        "threadId": "thread-test",
                        "turnId": "turn-test",
                        "itemId": "item-test",
                        "delta": "text.",
                    },
                }
            )
            send(
                {
                    "method": "turn/completed",
                    "params": {
                        "threadId": "thread-test",
                        "turn": {"id": "turn-test", "status": "completed"},
                    },
                }
            )
            if "--single-turn" in sys.argv:
                return


if __name__ == "__main__":
    main()
