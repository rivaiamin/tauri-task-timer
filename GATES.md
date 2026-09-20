# Gates: make the PRD's JIRA "fetch, save, view" claim true

OWNS: GATES.md, apps/tui/src/jira.rs, apps/tui/src/app/mod.rs, apps/tui/src/ui/timer_view.rs, apps/tui/src/ui/archive_view.rs, apps/tui/src/app/keymap.rs, scripts/verify-jira-open.mjs, docs/tui/prd.md, docs/tui/tasks.md, docs/tui/tech-spec.md, docs/tui/erd.md, apps/tui/README.md

Scope: a selected task with a JIRA key opens `{site}/browse/{KEY}` in the user's
browser from the TUI; the sprint/fetch-by-key ingest persists the JIRA status it
already fetches into `task_integrations` instead of discarding it; and the detail
overlay renders the stored integration rows, so the PRD's E4 "Task detail …,
Metadata in `task_integrations`" is observable rather than aspirational.

Context, recorded so the ledger is not read as a plan to weaken an oracle:

- The PRD (`docs/tui/prd.md:33`) lists "Replacing JIRA or Bitbucket (integrate,
  don't replicate)" as a non-goal, so the JIRA *view* is the browser's job. The
  TUI's job is the handoff, and `jira_site()` already resolves the host.
- `ingest_jira_issues` (`apps/tui/src/app/mod.rs:1048`) destructures the fetched
  status into `_status` and drops it, which is why `docs/tui/erd.md:121`'s
  `jira | status` row was documented but never written. Persisting it is the
  fix; the task's *own* `status` column stays owned by `auto_status`/the timer.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=767d47c7c35da852707024e32a5ac55ccfab36502df814bc5f3bb5db7634e522; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: a keyed task opens its JIRA issue in the browser, and an unkeyed one does not
  CHECK: node scripts/verify-jira-open.mjs
  EXPECT: jira open verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=c7cb9b804338f7dd340d4279b9fbd9a7cebc04a6a423499a3d5fa5e1e18204e4; exit=0; EXPECT=matched; output-sha256=0a678cfe4d11faa15576cdbac7692c55e12834ea71c0f073035f01556a8f6f0a; output-bytes=201; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the open oracle rejects a handoff that never happened
  CHECK: node scripts/verify-jira-open.mjs --self-test
  EXPECT: jira open control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=adf85184658becaccd4efd10557d7e208d5025c71a698932fd5c0855d1ce0513; exit=0; EXPECT=matched; output-sha256=c1646a92624930cc186d4bfa5afa914738e932619020ce405fabe4a3bb8ce955; output-bytes=196; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: the TUI crate builds and every unit test passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=bc0e547e7630bb9500a8d84400cbf661f8af629a8e5bd97ac7bf86d19e2b419d; output-bytes=5051; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: clippy reports no warnings on the TUI crate, all targets and features
  CHECK: node scripts/verify-clippy-clean.mjs
  EXPECT: clippy clean
  EVIDENCE: automatic-evidence=v1; definition-sha256=1e642a637ef1184752b89991d2181311373d43f334833c07e2498d9bbd739f65; exit=0; EXPECT=matched; output-sha256=f31ef79c253ce8c2fa97bf24a678530213886724c78c00b05d5b6acde6f94c28; output-bytes=13; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

<!--
G1 drives the real binary in a pty, with a fake `xdg-open` early on PATH that
records its argv, so the assertion reads the URL the TUI actually handed the
opener rather than the source text that builds it. Its second half presses `o`
on a keyless task and requires the capture file to stay unchanged, so the gate
measures the binding's discrimination rather than only its happy path.

G2 is the negative control: with no `xdg-open` on PATH the handoff cannot
happen, and the same predicate must reject it. Without that, a green G1 could
mean the opener never ran at all.

The sprint/fetch ingest is not driven in the pty: reaching it needs Ctrl+J,
which a terminal cannot distinguish from Enter. Its persistence is asserted by
`ingesting_a_jira_issue_stores_its_key_and_status` in apps/tui/src/app/mod.rs,
which G3 runs.

G3/G4 are the crate's own regression gates; the change adds a key binding, a
network-free URL builder, and a DB write, all covered there.
-->
