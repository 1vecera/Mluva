"""Minimal subprocess implementing the app-server frames used by contract tests."""

import json
import sys


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
