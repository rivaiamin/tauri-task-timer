# Gates: TUI clippy cleanup — no warnings under `-D warnings`, behavior unchanged

OWNS: GATES.md, apps/tui/src/, docs/tui/tasks.md, docs/tui/tech-spec.md, docs/tui/erd.md

Scope: `cargo clippy -p task-timer-tui --all-targets --all-features -- -D warnings`
exits zero, every existing TUI test still passes, and the existing behavioral
oracles (status sync, status picker, JIRA stop path) still pass — so the warning
cleanup changed no observable behavior. The pre-existing negative control (G4)
proves the status oracle can still fail.

Non-goals, recorded because the cleanup stops at them: `db/tasks.rs` keeps the
`total_time`, `position`, `link`, `end_time`, and `is_*` flag columns under
`#[allow(dead_code)]`. They are schema shared with the web app and are set and
read by SQL in this file, so deleting the struct fields would silently drop the
`done` flag the web dashboard writes. The `done` / `is_completed` split is a
pre-existing data bug in the web app (every row in `apps/web/local.db` has
`done = 1, is_completed = 0`) and is out of scope here.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=767d47c7c35da852707024e32a5ac55ccfab36502df814bc5f3bb5db7634e522; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: clippy reports no warnings on the TUI crate, all targets and features
  CHECK: ! cargo clippy -p task-timer-tui --all-targets --all-features -- -D warnings 2>&1 | grep -c 'warning:'
  EXPECT: /^0\s*$/
  EVIDENCE: automatic-evidence=v1; definition-sha256=bfb656f3cf4497b2de9d1f49a520bcc5ccea1ef5d90df89ca8bdbb387039c764; exit=0; EXPECT=matched; output-sha256=9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa; output-bytes=2; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the TUI crate builds and every unit test passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=65d57e58c160e6f4be39756559be241b49142cdce53ee6db6767eaea9935c50a; output-bytes=3371; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: the CLI status oracle still passes — start moves a task to In Progress, done moves it to the Done catalog id
  CHECK: cargo build -p task-timer-tui && node scripts/verify-status-sync.mjs --cli
  EXPECT: status sync verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=6c88d27a82f61db64a49713d528405cf2a71efca478395bafebb95908715cf49; exit=0; EXPECT=matched; output-sha256=d6fbd683cb9fed9744837923bdb9d3936b38d4d91d9ea11e4639c663a9d49557; output-bytes=194; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the status oracle's negative control still fails against pre-change behavior
  CHECK: node scripts/verify-status-sync.mjs --self-test
  EXPECT: status sync control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=374f6d53d2934b218506ca68599b558a65d5d13aacfd0c688c3190f28b35c513; exit=0; EXPECT=matched; output-sha256=4f2ae8fe17e9c3f4cc7119ae823e505071108e71bac70b36ea8f342c66f6e4f7; output-bytes=27; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: the picked status in a real TUI session is still what gets stored
  CHECK: node scripts/verify-status-picker.mjs
  EXPECT: status picker verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=b14d9caa2655dfc2efe3e1ec792b750330bfefa921891a775cb292e5ad5c58b7; exit=0; EXPECT=matched; output-sha256=1783f80d947e44d7ab55fee6b73f0772ce665530a5ab8d0996f31bbcac26015a; output-bytes=204; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G6: the prior JIRA stop to To Do behavior still holds
  CHECK: node scripts/verify-jira-stop.mjs
  EXPECT: stop verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=d6d9615d9e208ff3a21a658bcaa3d943ee8ce37eb3ea5cae4e0da00f2308c560; exit=0; EXPECT=matched; output-sha256=4233ef53302f6b525accfbad92a0c1f848e8e6d924760234d7be87f25a8946dc; output-bytes=195; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

<!--
G1 uses `-D warnings` so the gate cannot pass while a single warning remains; the
oracle is clippy's own exit code, not a warning count a human reads.
G3 rebuilds target/debug/task-timer-tui because G5 drives it over a pty.
-->
