# Gates: `jira ensure` — materialize a JIRA ticket as a timer task

OWNS: GATES-jira-ensure.md, apps/tui/src/cli.rs, apps/tui/src/db/tasks.rs, apps/tui/src/app/mod.rs, apps/tui/README.md, docs/tui/tech-spec.md, scripts/verify-jira-ensure.mjs

Scope: a headless `jira ensure KEY` that creates-or-reuses the timer task for a
JIRA ticket (identity labelled `KEY summary`, plus today's day row) and refreshes
its `jira/issue_key` and `jira/status` integration rows — idempotently, without
starting a timer and without duplicating an identity or a day row.

Context, recorded so this ledger is not read as a plan to weaken an oracle:

- The sprint runner writes agent state through `integration set`, which resolves a
  task **identity**. A ticket with no timer task is skipped as
  `integration-skipped reason=no-timer-task`, so agent badges and `status set` are
  meaningless for any key the operator has not hand-fetched.
- The runner is deliberately read-only on tasks: it must never create one. This
  command is the operator's/prep-script's explicit way to create the row, so the
  "no silent `add`" contract stays intact.
- It reuses what already ships: `jira::fetch_issue` (validates the key before any
  request), `db::tasks::create_task` (upsert by `idx_tasks_user_label` + ensure day
  row by `idx_task_days_task_date`), and `db::integrations::upsert` (keyed on
  task/group/field). No new store, no new table.
- The timer is NOT started: `start`/`stop` stay the E6 hook's and the operator's.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES-jira-ensure.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=3946da7fb7391fa89a39d3b36aca47fc3ea3ff25e5bf7c7ceda7d9ccc4dabcb8; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: `jira ensure` materializes the task from JIRA and a second run adds nothing
  CHECK: node scripts/verify-jira-ensure.mjs
  EXPECT: jira ensure verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=f49b4472c2a234b6b1fa72ab7b9f9f264505a7c6d7eb9c55df9ff6ae754df14c; exit=0; EXPECT=matched; output-sha256=ad559c38c5f9cbf00ea30739a1f8f3b8c134b54ba1fd87ae35a16f4734950da1; output-bytes=203; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the ensure oracle rejects a run that started the timer or moved its status
  CHECK: node scripts/verify-jira-ensure.mjs --self-test
  EXPECT: jira ensure control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=63db6f5b18b14beb39867d1ab007116725be221d4eb4640d1377e8d1afdf279a; exit=0; EXPECT=matched; output-sha256=bb0624cc1eed6a999266b245580e5b550993ff56ab830c01e12e2492a4c28274; output-bytes=310; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: a malformed key is rejected before any network call
  CHECK: node scripts/verify-jira-ensure.mjs --reject
  EXPECT: jira ensure rejects invalid keys
  EVIDENCE: automatic-evidence=v1; definition-sha256=55da7e83e673d289f45727c14729fe59f78d1c6232d1d54bc5fbbc4da0dff358; exit=0; EXPECT=matched; output-sha256=b14f18ccf8c221b595df835b9e0adca214e1ced5595b5d2e0c12d03af3a06d53; output-bytes=204; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the command is documented with its flags in the TUI README
  CHECK: node scripts/verify-jira-ensure.mjs --docs
  EXPECT: jira ensure documented
  EVIDENCE: automatic-evidence=v1; definition-sha256=417ac908f5bcc9ce87522ec7755a26c2735f7c58d0910015a83613f1b3e616c2; exit=0; EXPECT=matched; output-sha256=1c5d648ad34d6ed1279e98e5cc017576e7152163827b9330cf2ea73ea844f679; output-bytes=23; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: the TUI crate builds and every unit test passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=8dc253f95725355bbf2ed538ddd2e4e6789de48918cf652589bf00ff0a8fc37b; output-bytes=7919; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G6: clippy reports no warnings on the TUI crate, all targets and features
  CHECK: node scripts/verify-clippy-clean.mjs
  EXPECT: clippy clean
  EVIDENCE: automatic-evidence=v1; definition-sha256=1e642a637ef1184752b89991d2181311373d43f334833c07e2498d9bbd739f65; exit=0; EXPECT=matched; output-sha256=f31ef79c253ce8c2fa97bf24a678530213886724c78c00b05d5b6acde6f94c28; output-bytes=13; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G7: the TUI's timers still drive status and done through the day row
  CHECK: node scripts/verify-status-sync.mjs
  EXPECT: status sync verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=a8d3442788316b10f11b6d33d326ffd2629421cb1598be2a650c422ac821d5b4; exit=0; EXPECT=matched; output-sha256=ba70af49c2171c253b8ffdc91be101b4934c586424360713fbe38e8b66ce1b51; output-bytes=32; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G8: a ticket already in the timer is reused, not given a second identity
  CHECK: node scripts/verify-jira-ensure.mjs --reuse
  EXPECT: jira ensure reuses an existing task
  EVIDENCE: automatic-evidence=v1; definition-sha256=e7111713fb03e765c90e36fa3c48da1621ff50d831ac78dfb3408ee30a7b3390; exit=0; EXPECT=matched; output-sha256=d21ba727eb92128bbee1b1c89ad0351a7a68fe84da0ec03e2ca07e277394ed7e; output-bytes=207; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

<!--
G1 drives the real binary against a stub JIRA HTTP server (no credentials, no
network) over a throwaway DB, then measures the resulting rows from the DB
itself: one identity, one day row, and the two `jira/*` integration rows. It then
re-runs the command and requires the counts to be unchanged — that is the
idempotency claim, measured rather than asserted in prose.

G2 is the negative control: it runs the same predicate against a fixture with a
duplicated identity/day row and requires the predicate to reject it. Without it a
green G1 could mean the predicate never looked.

G3 is the key-validation arm: `fetch_issue` validates the key shape before any
request, so an invalid key must exit non-zero with no row written. The stub server
is not started for this arm, so a network attempt would hang rather than pass.

G5 must run after a rebuild; a stale `target/debug` binary silently tests the
previous CLI surface.
-->
