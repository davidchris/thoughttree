#!/usr/bin/env python3
"""Deterministic, offline ACP adapter for desktop integration tests and screenshots.

This program never calls a model, accesses the network, or reads Vault contents.
Use it as the configured Codex adapter. Set CODEX_PATH to this same file to route
ephemeral summaries here as well. Every generated response identifies the fixture.
"""

import json
import os
import sys
from pathlib import Path


SESSION = "fixture-session"
MODELS = [
    {"value": "fixture-model", "name": "Deterministic fixture"},
    {"value": "fixture-alternate", "name": "Alternate fixture"},
]


def send(payload):
    print(json.dumps({"jsonrpc": "2.0", **payload}), flush=True)


def result(request_id, value):
    send({"id": request_id, "result": value})


def update(value):
    send({"method": "session/update", "params": {"sessionId": SESSION, "update": value}})


def text(content):
    update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": content}})


def config_options(model="fixture-model"):
    return [{"id": "model", "name": "Model", "category": "model", "type": "select",
             "currentValue": model, "options": MODELS}]


def tool(tool_id, title, kind, status, locations=None):
    value = {"sessionUpdate": "tool_call", "toolCallId": tool_id,
             "title": title, "kind": kind, "status": status}
    if locations is not None:
        value["locations"] = [{"path": path} for path in locations]
    update(value)


def finish_prompt(request_id, option="not_requested"):
    if option != "not_requested":
        update({"sessionUpdate": "tool_call_update", "toolCallId": "fixture-fetch",
                "status": "completed" if option == "allow-once" else "failed"})
    text(f"\n\n## Deterministic fixture response\n\nPermission outcome: `{option}`. "
         "This is synthetic test data; no external request was made.\n\n")
    text("| Capability | Result |\n| --- | --- |\n| Streaming | Ordered chunks |\n"
         "| Context | Vault file reference |\n| Permissions | Explicit user choice |\n\n"
         "```rust\nlet native = true;\n```\n\n"
         "The two branches combine as $a^2 + b^2 = c^2$.\n\n"
         "```mermaid\ngraph TD\n A[Question] --> B[Branch one]\n A --> C[Branch two]\n B --> D[Synthesis]\n C --> D\n```\n")
    result(request_id, {"stopReason": "end_turn"})


def prompt(request):
    content = "\n".join(part.get("text", "") for part in request["params"].get("prompt", [])
                        if part.get("type") == "text")
    configured = json.loads(os.environ.get("CODEX_CONFIG", "{}"))
    text("Deterministic ACP fixture — synthetic desktop parity demonstration.\n\n")
    text(f"Model: `{configured.get('model', 'fixture-model')}` · "
         f"Reasoning effort: `{configured.get('model_reasoning_effort', 'default')}`.\n")
    tool("fixture-read", "Read fixture-notes.md (synthetic)", "read", "completed",
         [str(Path.cwd() / "fixture-notes.md")])
    if "fixture:fail" in content:
        tool("fixture-incomplete", "Synthetic unfinished operation", "search", "in_progress")
        send({"id": request["id"], "error": {"code": -32000, "message": "Deterministic fixture failure"}})
        return None
    if "fixture:nopermission" in content:
        finish_prompt(request["id"])
        return None
    tool("fixture-fetch", "WebFetch example.org (fixture; no network)", "fetch", "pending")
    send({"id": "fixture-permission", "method": "session/request_permission", "params": {
        "sessionId": SESSION,
        "toolCall": {"toolCallId": "fixture-fetch", "title": "WebFetch example.org (fixture; no network)",
                     "kind": "fetch", "status": "pending"},
        "options": [
            {"optionId": "allow-once", "name": "Allow once", "kind": "allow_once"},
            {"optionId": "reject-once", "name": "Deny", "kind": "reject_once"},
        ],
    }})
    # Sent after the request, while it remains unanswered: integration tests
    # verify the client's dispatch loop continues processing notifications.
    text("\nAwaiting your choice — this permission is parked without a timeout.\n")
    return request["id"]


def main():
    if "--version" in sys.argv:
        print("codex-acp deterministic fixture 1.0 (offline)")
        return
    if "exec" in sys.argv:
        # Codex summaries use exec --ephemeral instead of ACP session/new.
        sys.stdin.read()
        print("Deterministic Fixture Response")
        return
    pending = None
    for line in sys.stdin:
        request = json.loads(line)
        method = request.get("method")
        if method == "initialize":
            result(request["id"], {"protocolVersion": 1, "agentCapabilities": {},
                                   "agentInfo": {"name": "offline-parity-fixture", "version": "1.0"}})
        elif method == "session/new":
            result(request["id"], {"sessionId": SESSION, "configOptions": config_options()})
        elif method in ("session/set_config_option", "session/set_model"):
            selected = request["params"].get("value", request["params"].get("modelId", "fixture-model"))
            result(request["id"], {"configOptions": config_options(selected)})
        elif method == "session/prompt":
            pending = prompt(request)
        elif request.get("id") == "fixture-permission" and pending is not None:
            outcome = request.get("result", {}).get("outcome", {})
            finish_prompt(pending, outcome.get("optionId", "cancelled"))
            pending = None
        elif method is not None and "id" in request:
            send({"id": request["id"], "error": {"code": -32601, "message": f"Unsupported fixture method: {method}"}})


if __name__ == "__main__":
    main()
