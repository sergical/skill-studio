# ============================================================================
# Skill Studio - agent benchmark harness adapters
# How to start each agent CLI headless and isolated, and how to read its
# output (answer text, time, tokens, cost, tool calls).
# ============================================================================
"""One adapter per agent CLI.

Each adapter gives `command(run) -> (argv, env)` and `parse(stdout) -> dict`.
Isolation rules (checked by hand, see METHODOLOGY.md):

- Every agent gets a fresh runtime HOME, so `~` in a shell command cannot
  reach the user's real skills, logs or config.
- Codex, OpenCode and pi find their auth through a link in that home.
- Claude Code keeps its sign-in in the macOS keychain, which it finds through
  `$HOME/Library/Keychains`, and its account in `~/.claude.json`. The runtime
  home gets a link to the keychain folder and a `.claude.json` with only the
  account keys. The flags `--setting-sources project`, `--strict-mcp-config`
  and `--disable-slash-commands` stop it from loading skills, settings and MCP
  servers.
"""

from __future__ import annotations

import json
import os
import shutil
from dataclasses import dataclass
from pathlib import Path

# MCP tools that only read. `invocation` also writes when it gets a policy;
# the prompt forbids that, the fixture is read-only and the hash check
# catches any change.
MCP_READ_TOOLS = [
    "scan", "diagnose", "diagnose_conflict", "doctor", "extras", "list_events",
    "outdated", "preview_frontmatter_repair", "review_due", "skill_usage",
    "turn_off_check", "invocation",
]
MCP_WRITE_TOOLS = [
    "add", "apply_frontmatter_repair", "fix", "park", "remove", "restore_event",
    "review_clear", "review_set", "split", "turn_off_for_agent", "unpark",
    "update", "update_all",
]
SHELL_TOOLS_CLAUDE = ["Bash", "Read", "Glob", "Grep"]
DENIED_CLAUDE = ["Edit", "Write", "NotebookEdit", "Agent", "WebFetch", "WebSearch"]


@dataclass
class Run:
    harness: str
    model: str
    method: str
    prompt: str
    cwd: Path
    runtime: Path
    home: Path
    skill_studio: Path
    wrapper_dir: Path


def which(name: str, *fallbacks: str) -> str:
    for candidate in (os.environ.get(f"BENCH_{name.upper().replace('-', '_')}_BIN"), shutil.which(name), *fallbacks):
        if candidate and Path(candidate).exists():
            return candidate
    raise SystemExit(f"{name} not found. Set BENCH_{name.upper().replace('-', '_')}_BIN to its path.")


def skill_studio_env(run: Run) -> dict[str, str]:
    """Environment for the Skill Studio CLI and MCP server: read the fixture, not the real home."""
    return {
        "SKILL_STUDIO_HOME": str(run.home),
        "XDG_DATA_HOME": str(run.home / ".local" / "share"),
        "XDG_CONFIG_HOME": str(run.home / ".config"),
        # Skill Studio honours CODEX_HOME even with --home, and the Codex
        # harness sets it to the agent's own runtime folder.
        "CODEX_HOME": str(run.home / ".codex"),
        "SKILL_STUDIO_TELEMETRY": "0",
    }


def write_cli_wrapper(run: Run) -> None:
    """A `skill-studio` on PATH that always reads the fixture home."""
    run.wrapper_dir.mkdir(parents=True, exist_ok=True)
    env = " ".join(f'{k}="{v}"' for k, v in skill_studio_env(run).items())
    script = f"""#!/bin/sh
export {env}
unset OPENCODE_DB OPENCODE_CONFIG_DIR CLAUDE_CONFIG_DIR
BIN="{run.skill_studio}"
for arg in "$@"; do case "$arg" in --home|--home=*) exec "$BIN" "$@" ;; esac; done
case "$1" in
  ""|-*|help|mcp) exec "$BIN" "$@" ;;
  review) sub="$1"; shift; case "$1" in ""|-*) exec "$BIN" "$sub" "$@" ;; esac
          act="$1"; shift; exec "$BIN" "$sub" "$act" --home "{run.home}" "$@" ;;
  *) sub="$1"; shift; exec "$BIN" "$sub" --home "{run.home}" "$@" ;;
esac
"""
    path = run.wrapper_dir / "skill-studio"
    path.write_text(script)
    path.chmod(0o755)


def base_env(run: Run) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in ("XDG_DATA_HOME", "OPENCODE_DB", "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT")}
    if run.method == "cli":
        write_cli_wrapper(run)
        env["PATH"] = f"{run.wrapper_dir}:{env['PATH']}"
    return env


def link_auth(src: Path, dst: Path) -> None:
    if not src.exists():
        raise SystemExit(f"Auth file missing: {src}. Log in to the agent first.")
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.symlink_to(src)


def mcp_server(run: Run) -> dict:
    return {"command": str(run.skill_studio), "args": ["mcp"], "env": skill_studio_env(run)}


# ----------------------------------------------------------------------------
# Claude Code
# ----------------------------------------------------------------------------


def claude_command(run: Run):
    config = {"mcpServers": {"skill-studio": mcp_server(run)} if run.method == "mcp" else {}}
    config_path = run.runtime / "mcp.json"
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text(json.dumps(config))
    allowed = SHELL_TOOLS_CLAUDE + ([f"mcp__skill-studio__{t}" for t in MCP_READ_TOOLS] if run.method == "mcp" else [])
    denied = DENIED_CLAUDE + [f"mcp__skill-studio__{t}" for t in MCP_WRITE_TOOLS]
    argv = [
        which("claude"), "-p", run.prompt, "--model", run.model,
        "--output-format", "stream-json", "--verbose", "--no-session-persistence",
        "--setting-sources", "project", "--disable-slash-commands",
        "--strict-mcp-config", "--mcp-config", str(config_path),
        "--permission-mode", "dontAsk", "--max-budget-usd", "8",
        "--allowedTools", *allowed, "--disallowedTools", *denied,
    ]
    env = base_env(run)
    env["HOME"] = str(claude_home(run))
    return argv, env


CLAUDE_ACCOUNT_KEYS = ("oauthAccount", "userID", "hasCompletedOnboarding", "lastOnboardingVersion")


def claude_home(run: Run) -> Path:
    home = run.runtime / "home"
    (home / "Library").mkdir(parents=True, exist_ok=True)
    (home / "Library" / "Keychains").symlink_to(Path.home() / "Library" / "Keychains")
    account = json.loads((Path.home() / ".claude.json").read_text())
    (home / ".claude.json").write_text(json.dumps({k: account[k] for k in CLAUDE_ACCOUNT_KEYS if k in account}))
    return home


def claude_parse(lines: list[dict]) -> dict:
    out = {"answer": "", "tool_calls": 0, "turns": 0, "cost_usd": None, "cost_source": "harness", "tokens": {}}
    for m in lines:
        if m.get("type") == "assistant":
            out["tool_calls"] += sum(1 for b in m.get("message", {}).get("content", []) if b.get("type") == "tool_use")
        elif m.get("type") == "result":
            u = m.get("usage", {})
            out.update(
                answer=m.get("result") or "",
                turns=m.get("num_turns", 0),
                cost_usd=m.get("total_cost_usd"),
                error=None if not m.get("is_error") else m.get("subtype") or "error",
                tokens={
                    "input": u.get("input_tokens", 0),
                    "cache_read": u.get("cache_read_input_tokens", 0),
                    "cache_write": u.get("cache_creation_input_tokens", 0),
                    "output": u.get("output_tokens", 0),
                },
            )
    return out


# ----------------------------------------------------------------------------
# Codex
# ----------------------------------------------------------------------------


def codex_command(run: Run):
    codex_home = run.runtime / ".codex"
    link_auth(Path.home() / ".codex" / "auth.json", codex_home / "auth.json")
    # The `codex` on PATH can be a package-manager shim that fails under a
    # moved HOME; BENCH_CODEX_BIN points at the native binary.
    argv = [
        which("codex"), "exec", "--json", "--ephemeral", "--ignore-user-config", "--ignore-rules",
        "--skip-git-repo-check", "--sandbox", "workspace-write", "-C", str(run.cwd),
        "--add-dir", str(run.home / ".skill-studio"),
        "--disable", "plugins", "--disable", "apps", "-m", run.model,
    ]
    if run.method == "mcp":
        server = mcp_server(run)
        argv += [
            "-c", f"mcp_servers.skill-studio.command={json.dumps(server['command'])}",
            "-c", 'mcp_servers.skill-studio.args=["mcp"]',
            "-c", "mcp_servers.skill-studio.env={" + ",".join(f"{k}={json.dumps(v)}" for k, v in server["env"].items()) + "}",
            "-c", f"mcp_servers.skill-studio.enabled_tools={json.dumps(MCP_READ_TOOLS)}",
            # `invocation` has no read-only hint, and `codex exec` never asks, so it would always fail.
            "-c", 'mcp_servers.skill-studio.tools.invocation.approval_mode="approve"',
        ]
    argv.append(run.prompt)
    env = base_env(run)
    env.update(HOME=str(run.runtime), CODEX_HOME=str(codex_home))
    return argv, env


def codex_parse(lines: list[dict]) -> dict:
    out = {"answer": "", "tool_calls": 0, "turns": 0, "cost_usd": None, "cost_source": "estimate", "tokens": {"input": 0, "cache_read": 0, "cache_write": 0, "output": 0}}
    for m in lines:
        kind = m.get("type")
        item = m.get("item") or {}
        if kind == "item.completed":
            if item.get("type") == "agent_message":
                out["answer"] = item.get("text", "")
            elif item.get("type") not in ("reasoning", "error", "todo_list"):
                out["tool_calls"] += 1
        elif kind == "turn.completed":
            out["turns"] += 1
            u = m.get("usage", {})
            t = out["tokens"]
            cached = u.get("cached_input_tokens", 0)
            t["input"] += u.get("input_tokens", 0) - cached
            t["cache_read"] += cached
            t["cache_write"] += u.get("cache_write_input_tokens", 0)
            # output_tokens already includes reasoning_output_tokens.
            t["output"] += u.get("output_tokens", 0)
        elif kind in ("turn.failed", "error"):
            out["error"] = json.dumps(m.get("error") or m)[:300]
    return out


# ----------------------------------------------------------------------------
# OpenCode (models through OpenRouter)
# ----------------------------------------------------------------------------


def opencode_command(run: Run):
    data = run.runtime / ".local" / "share"
    link_auth(Path.home() / ".local" / "share" / "opencode" / "auth.json", data / "opencode" / "auth.json")
    config: dict = {
        "$schema": "https://opencode.ai/config.json",
        "autoupdate": False,
        "share": "disabled",
        "permission": {"skill": "deny", "edit": "deny", "webfetch": "deny", "bash": "allow", "external_directory": "allow"},
    }
    if run.method == "mcp":
        server = mcp_server(run)
        # Code Mode (the default) wraps every MCP tool in one `execute` tool, which the per-tool deny list below cannot reach.
        config["mcp"] = {"skill-studio": {"type": "local", "command": [server["command"], *server["args"]], "environment": server["env"], "enabled": True, "codemode": False}}
        config["tools"] = {f"skill-studio_{t}": False for t in MCP_WRITE_TOOLS}
    argv = [which("opencode"), "run", "--format", "json", "-m", f"openrouter/{run.model}", run.prompt]
    env = base_env(run)
    env.update(
        HOME=str(run.runtime),
        XDG_DATA_HOME=str(data),
        XDG_CONFIG_HOME=str(run.runtime / ".config"),
        XDG_STATE_HOME=str(run.runtime / ".local" / "state"),
        OPENCODE_DISABLE_CLAUDE_CODE="1",
        OPENCODE_DISABLE_PROJECT_CONFIG="1",
        OPENCODE_DISABLE_AUTOUPDATE="1",
        OPENCODE_CONFIG_CONTENT=json.dumps(config),
    )
    return argv, env


def opencode_parse(lines: list[dict]) -> dict:
    out = {"answer": "", "tool_calls": 0, "turns": 0, "cost_usd": None, "cost_source": "estimate", "tokens": {"input": 0, "cache_read": 0, "cache_write": 0, "output": 0, "reasoning": 0}}
    for m in lines:
        kind = m.get("type")
        part = m.get("part") or {}
        if kind == "text":
            out["answer"] = part.get("text", "")
        elif kind == "tool_use":
            out["tool_calls"] += 1
        elif kind == "step_finish":
            out["turns"] += 1
            tk = part.get("tokens", {})
            t = out["tokens"]
            t["input"] += tk.get("input", 0)
            t["output"] += tk.get("output", 0)
            t["reasoning"] += tk.get("reasoning", 0)
            t["cache_read"] += tk.get("cache", {}).get("read", 0)
            t["cache_write"] += tk.get("cache", {}).get("write", 0)
        elif kind == "error":
            out["error"] = json.dumps(m.get("error") or m)[:300]
    return out


# ----------------------------------------------------------------------------
# pi (models through OpenRouter)
# ----------------------------------------------------------------------------


def pi_command(run: Run):
    agent_dir = run.runtime / ".pi" / "agent"
    link_auth(Path.home() / ".pi" / "agent" / "auth.json", agent_dir / "auth.json")
    tools = ["read", "bash", "grep", "find", "ls"]
    argv = [which("pi"), "-p", "--mode", "json", "--no-session", "--no-skills", "--no-context-files", "--no-extensions", "--model", f"openrouter/{run.model}"]
    if run.method == "mcp":
        # pi's default "codemode" exposure hides MCP tools behind one script tool.
        server = {**mcp_server(run), "exposure": "direct"}
        # pi turns "-" in a server name into "_" in its tool names.
        (agent_dir / "mcp.json").write_text(json.dumps({"mcpServers": {"skill_studio": server}}))
        tools += [f"mcp__skill_studio__{t}" for t in MCP_READ_TOOLS]
        argv += ["-e", "builtin:mcp"]
    else:
        argv.append("--no-mcp")
    argv += ["--tools", ",".join(tools), run.prompt]
    env = base_env(run)
    env.update(HOME=str(run.runtime), PI_CODING_AGENT_DIR=str(agent_dir))
    return argv, env


def pi_parse(lines: list[dict]) -> dict:
    out = {"answer": "", "tool_calls": 0, "turns": 0, "cost_usd": 0.0, "cost_source": "harness", "tokens": {"input": 0, "cache_read": 0, "cache_write": 0, "output": 0, "reasoning": 0}}
    for m in lines:
        if m.get("type") != "message_end":
            continue
        msg = m.get("message") or {}
        if msg.get("role") != "assistant":
            continue
        out["turns"] += 1
        content = msg.get("content") or []
        out["tool_calls"] += sum(1 for b in content if b.get("type") == "toolCall")
        text = "".join(b.get("text", "") for b in content if b.get("type") == "text")
        if text.strip():
            out["answer"] = text
        u = msg.get("usage") or {}
        t = out["tokens"]
        t["input"] += u.get("input", 0)
        t["output"] += u.get("output", 0)
        t["cache_read"] += u.get("cacheRead", 0)
        t["cache_write"] += u.get("cacheWrite", 0)
        t["reasoning"] += u.get("reasoning", 0)
        out["cost_usd"] += (u.get("cost") or {}).get("total", 0)
        if msg.get("stopReason") == "error":
            out["error"] = (msg.get("errorMessage") or "error")[:300]
    return out


ADAPTERS = {
    "claude": (claude_command, claude_parse),
    "codex": (codex_command, codex_parse),
    "opencode": (opencode_command, opencode_parse),
    "pi": (pi_command, pi_parse),
}
