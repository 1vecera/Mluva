# Guarded Herdr prompt dependency

[guarded-prompts.patch](guarded-prompts.patch) is a separately reviewable Apache-2.0 patch against upstream Herdr **0.9.5**, commit `143bf5fd4781d164260c63fe28e7dc08054ad32f`. The reviewed local patch commit is `4fc933fcf8891d7ec44dc5a33ab62013eb09f68f`. It is included here because upstream requires prior maintainer agreement for external implementation PRs; no upstream issue or PR was submitted.

This patch adds three JSON API methods. Existing prompt behavior and the frozen client/server wire enums are unchanged. The generated JSON schema includes the new methods; old clients never request their new response types.

| Method | Contract |
| --- | --- |
| `agent.guarded_list` | Returns a per-process server epoch and opaque identity guards for ready native Codex/Claude sessions. Guards bind terminal, pane, name, native session, foreground process, process start and process group. |
| `agent.prompt_guarded` | Requires that epoch, guard, exact pane, operation UUID, expiry and bounded final text. Validation occurs in the server owner, then before every partial text/Enter write in the PTY owner. Rejects blockers, non-agent shells, control sequences and expired requests. |
| `agent.prompt_receipt` | Looks up one operation without sending. States are pending, accepted, rejected and uncertain. Accepted means completed PTY text/Enter writes, never completed agent work. |

An in-memory ten-minute ledger holds at most 1,024 hashed requests, with eight pending workers. Repeating an identical operation returns its receipt; reusing its ID with another payload is rejected. Mluva sends only once and reconciles by receipt reads. Restart changes the epoch and destroys the ledger; clients must never replay an uncertain operation after restart. A post-queue error is uncertain because partial input may have been written. Rejecting a guarded write removes only that submission: the PTY and recipient stay running.

The Linux proof uses actual foreground process/start checks and a native detection snapshot before input writes. It cannot retract already written bytes or make a PTY write equivalent to a correlated native agent turn. No process is frozen or stopped to perform validation. Future native session prompt/receipt APIs could strengthen that boundary without changing Mluva's interaction.

Directly launched Codex sessions are also eligible when their native session identity is known and their current composer is empty. They do not need to be restarted through Herdr's managed launcher. Pending launches, nonempty composers and missing identities remain excluded; every submission retains the process, session, epoch, expiry and PTY checks. Directly launched Claude sessions still require managed readiness.

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
```

Before an approved installation, preserve the existing Herdr executable/config and use its documented session migration procedure. A debug build uses `herdr-dev` configuration by default; a release build uses the normal application directory. An explicit local socket in Mluva can address either. Do not point a disposable proof at the live server.

Native session identity requires Herdr's Codex/Claude integration. Codex 0.160.1 asks its user to review new hooks. Mluva does not approve that prompt or work around missing identity. The isolated transport test uses a fresh Codex session with automatic hooks disabled and replays its actual session metadata through the reviewed built-in Herdr hook; it does not establish live hook-trust acceptance. Claude Code 2.1.301's built-in hook runs normally in the private fixture.
