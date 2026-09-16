# Gates: TUI status — picker + automatic In Progress / done sync

OWNS: GATES.md, scripts/verify-status-sync.mjs, scripts/tui-driver.mjs, apps/tui/src/, apps/tui/README.md, docs/api.md, docs/tui/epics.md, docs/tui/tasks.md, docs/tui/tech-spec.md

Scope: The status field in the TUI edit form is chosen from the JIRA catalog
instead of typed, and a task's status follows its state — starting a timer moves
it to In Progress, checking it done moves it to Done (Cek di Local), in the TUI
and in the CLI that shares its timer core.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=767d47c7c35da852707024e32a5ac55ccfab36502df814bc5f3bb5db7634e522; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: starting a (non-JIRA) task moves its stored status to the In Progress catalog id, and checking it done moves it to the Done catalog id
  CHECK: cargo build -p task-timer-tui && node scripts/verify-status-sync.mjs --cli
  EXPECT: status sync verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=6c88d27a82f61db64a49713d528405cf2a71efca478395bafebb95908715cf49; exit=0; EXPECT=matched; output-sha256=6379ed6946712012f9f7321d7a6b0d9003193ac7d8d2e290e19c03fbbfbc192c; output-bytes=3086; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the same sync oracle fails against the pre-change binary (negative control)
  CHECK: node scripts/verify-status-sync.mjs --self-test
  EXPECT: status sync control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=374f6d53d2934b218506ca68599b558a65d5d13aacfd0c688c3190f28b35c513; exit=0; EXPECT=matched; output-sha256=4f2ae8fe17e9c3f4cc7119ae823e505071108e71bac70b36ea8f342c66f6e4f7; output-bytes=27; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: in a real TUI session, the picked status in the edit form is what gets stored, and Start / Done move the status on screen without any edit
  CHECK: node scripts/verify-status-picker.mjs
  EXPECT: status picker verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=b14d9caa2655dfc2efe3e1ec792b750330bfefa921891a775cb292e5ad5c58b7; exit=0; EXPECT=matched; output-sha256=7300b5b74122c405b6ea15daf0cf21197372d5078f600497f1025a50634d14ba; output-bytes=205; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the TUI crate builds and its unit tests pass
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=b102c0a03b5b7d1872fdcf00a3ccec5efb87b21f8f0a8d5812db69e9ef02c2b5; output-bytes=6423; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: the prior JIRA stop→To Do behavior still holds
  CHECK: node scripts/verify-jira-stop.mjs
  EXPECT: stop verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=d6d9615d9e208ff3a21a658bcaa3d943ee8ce37eb3ea5cae4e0da00f2308c560; exit=0; EXPECT=matched; output-sha256=f1cae8f87c7ad7f793fea17149866dea42c12a71adb8eb02c2153e29231b6707; output-bytes=196; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

<!--
G1's CHECK rebuilds the binary because G2/G3 drive target/debug/task-timer-tui; the
cargo-test verdict is G4's, not G1's. G3 drives the real TUI over the hook socket
and a pty (scripts/verify-status-picker.mjs + scripts/tui-driver.mjs).
-->
