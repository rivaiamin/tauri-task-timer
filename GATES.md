# Gates: TUI JIRA stop → To Do

OWNS: GATES.md, scripts/verify-jira-stop.mjs, apps/tui/src/jira.rs, apps/tui/src/app/mod.rs, apps/tui/src/cli.rs, apps/tui/src/app/keymap.rs, docs/api.md

Scope: In the TUI (and the CLI that shares its timer core), stopping a running
JIRA-linked task logs the run and returns the issue to To Do; marking the task
done keeps the issue at the done status instead.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=767d47c7c35da852707024e32a5ac55ccfab36502df814bc5f3bb5db7634e522; exit=0; EXPECT=matched; output-sha256=b2615d84cb888cfab30603922689407564a0eec8b8aa9cc9782c6eb9af862464; output-bytes=268; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: a stopped (not done) task logs work and transitions the issue to the To Do status
  CHECK: node scripts/verify-jira-stop.mjs
  EXPECT: stop verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=d6d9615d9e208ff3a21a658bcaa3d943ee8ce37eb3ea5cae4e0da00f2308c560; exit=0; EXPECT=matched; output-sha256=0b9609a08723892e726b1202888f38063a7850c1e6f5484b0616ae2b0bc1ba69; output-bytes=196; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the stop verifier fails against a worklog-only stop oracle (negative control)
  CHECK: node scripts/verify-jira-stop.mjs --self-test
  EXPECT: stop oracle control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=6d012ec9901606222715a102394dcfb53f1301a56a554ee8a3778e47a262d321; exit=0; EXPECT=matched; output-sha256=09b11f0715825be7208a50640912ad07c5a7fd7541b4e2b6dce754bf0bd696c6; output-bytes=198; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: every stop path in the TUI routes through the stop oracle; every done path still reaches the done status
  CHECK: node scripts/verify-jira-stop.mjs --paths
  EXPECT: stop path coverage passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=df64638884b96555d2a6c1069a97821290f176cf4c330627c15578841a0d0247; exit=0; EXPECT=matched; output-sha256=84bb0d6defc13db5b085f018da643ede7d1c601c3f2a644c832075a5f6e082f7; output-bytes=26; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the TUI crate builds and its unit tests pass
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=58c87fe09a24a4fd762e25972a6d9c02ba8b98ba4b1ac9a1d773968727a7228f; output-bytes=5465; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: docs/api.md documents the stop→To Do effect instead of "status unchanged"
  EVIDENCE: docs/api.md:128 reads `| **stop** a task | worklog for the run, then issue → **To Do** |`; `grep -n "status unchanged" docs/api.md` returns no match. Rows also added for hook pause / session end, plus a paragraph stating a stop only returns to To Do when the task is not checked done.

- [x] G6: a live TUI session moves a real ticket on start and back on stop (requires JIRA credentials + a scratch ticket)
  EVIDENCE: Against US-1827 on aimsis.atlassian.net with real credentials. Baseline: status `To Do` (id 10000), 8 worklogs, last id 12219. `start` → POST transition 21 → API status `In Progress`. `stop` (not done) → POST worklog 12347 (1m) then POST transition 11 → API status `To Do` (the requested behavior). `done` after a second start → API status `Cek di Local` (id 10004), not To Do (the "unless checked as done" exception holds). Restored: deleted worklogs 12347 + 12349 (HTTP 204 each) and POSTed transition 11; final status `To Do`, 8 worklogs, last id 12219 — identical to baseline.

<!--
G6 is manual on purpose: it is the only gate that observes the real JIRA
workflow's transition graph, which a local oracle cannot reproduce. G1–G3 use a
local HTTP stub, so they prove our request sequence, not JIRA's acceptance.
-->
