# Guarded Herdr prompt dependency

[guarded-prompts.patch](guarded-prompts.patch) is a separately reviewable Apache-2.0 patch against upstream Herdr **0.9.5**, commit `143bf5fd4781d164260c63fe28e7dc08054ad32f`. The reviewed local patch commit is `ff8b76017d3e6a967cdba233b102ddd79513b869`. It is included here because upstream requires prior maintainer agreement for external implementation PRs; no upstream issue or PR was submitted.

This patch adds three JSON API methods. Existing prompt behavior and the frozen client/server wire enums are unchanged. The generated JSON schema includes the new methods; old clients never request their new response types.

| Method | Contract |
| --- | --- |
| `agent.guarded_list` | Returns a per-process server epoch and opaque identity guards for ready native Codex/Claude sessions. Guards bind terminal, pane, name, native session, foreground process, process start and process group. |
| `agent.prompt_guarded` | Requires that epoch, guard, exact pane, operation UUID, expiry and bounded final text. Validation occurs in the server owner, then before every partial text/Enter write in the PTY owner. Rejects blockers, non-agent shells, control sequences, expired requests and Claude Code composers outside vim INSERT mode. |
| `agent.prompt_receipt` | Looks up one operation without sending. States are pending, accepted, rejected and uncertain. Accepted means completed PTY text/Enter writes, never completed agent work. |

An in-memory ten-minute ledger holds at most 1,024 hashed requests, with eight pending workers. Repeating an identical operation returns its receipt; reusing its ID with another payload is rejected. Mluva sends only once and reconciles by receipt reads. Restart changes the epoch and destroys the ledger; clients must never replay an uncertain operation after restart. A post-queue error is uncertain because partial input may have been written. Rejecting a guarded write removes only that submission: the PTY and recipient stay running.

The Linux proof uses actual foreground process/start checks and a native detection snapshot before input writes. It cannot retract already written bytes or make a PTY write equivalent to a correlated native agent turn. No process is frozen or stopped to perform validation. Future native session prompt/receipt APIs could strengthen that boundary without changing Mluva's interaction.

Directly launched Codex sessions are also eligible when their native session identity is known and their current composer is empty. Directly launched Claude Code sessions are eligible when their native session came from Herdr's own Claude Code hook, Herdr detects Claude Code as the pane's foreground agent, and both Herdr's published state and the current screen show it idle or working with no blocker. Neither needs to be restarted through Herdr's managed launcher; managed sessions keep their launch readiness. Pending launches, nonempty Codex composers, missing identities and blocked sessions remain excluded; every submission retains the process, session, epoch, expiry and PTY checks.

Claude Code's vim NORMAL mode draws no indicator, exactly like vim mode off, so Herdr remembers when the current Claude Code process has drawn `-- INSERT --` or `-- VISUAL --`. Such a process, managed or not, must show `-- INSERT --` before the first byte of a prompt; between the paste and Enter, Claude Code's own paste notice may replace it. VISUAL mode is always refused. A refusal before the first write is a rejected receipt and Mluva keeps the draft. A process that never shows an indicator, because vim mode is off or `hideVimModeIndicator` is set, is sent to as before.

## Prepare an isolated build

Review the patch first. These commands prepare a new checkout and executable; they do not install it, restart Herdr, migrate live sessions or alter desktop settings. Use the upstream Rust toolchain and Zig **0.16.0** as required by that checkout.

```sh
git -C /path/to/herdr fetch origin
git -C /path/to/herdr worktree add -b integration/mluva-guarded-prompts \
  /path/to/herdr/.claude/worktrees/mluva-guarded-prompts \
  143bf5fd4781d164260c63fe28e7dc08054ad32f
cd /path/to/herdr/.claude/worktrees/mluva-guarded-prompts
git apply --check /path/to/Mluva/integrations/herdr/guarded-prompts.patch
git apply /path/to/Mluva/integrations/herdr/guarded-prompts.patch
cargo test --locked guarded
cargo test --locked api::schema
cargo test --locked protocol::wire
cargo test --locked pty::actor
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --locked
cd /path/to/Mluva
bash linux/tests/run_herdr_voice_smoke.sh /absolute/patched/target/debug/herdr
bash linux/tests/run_herdr_voice_smoke.sh /absolute/patched/target/debug/herdr --manual-codex
bash linux/tests/run_herdr_voice_smoke.sh /absolute/patched/target/debug/herdr --manual-claude
```

Before an approved installation, preserve the existing Herdr executable/config and use its documented session migration procedure. A debug build uses `herdr-dev` configuration by default; a release build uses the normal application directory. An explicit local socket in Mluva can address either. Do not point a disposable proof at the live server.

Native session identity requires Herdr's Codex/Claude integration. Codex 0.160.1 asks its user to review new hooks. Mluva does not approve that prompt or work around missing identity. The isolated transport test uses a fresh Codex session with automatic hooks disabled and replays its actual session metadata through the reviewed built-in Herdr hook; it does not establish live hook-trust acceptance. Claude Code's built-in hook runs normally in the private fixture, last checked with 2.1.306. With `--manual-claude`, Claude Code is typed into its pane with vim key bindings; the proof checks that NORMAL mode is refused and that a multi-line prompt is accepted after returning to INSERT.
