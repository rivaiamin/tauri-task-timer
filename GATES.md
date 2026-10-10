# Gates: one task per label, one row per day worked

OWNS: GATES.md, apps/web/drizzle/0004_superb_tag.sql, apps/web/src/lib/server/**, apps/web/src/routes/**, apps/tui/src/**, apps/mcp/**, packages/shared/**, scripts/verify-day-identity.mjs, docs/**, README.md, apps/tui/README.md, apps/mcp/README.md

Scope: the same label is one task across every day it is worked; each day keeps
its own recorded time; the archive shows one entry per task with a per-day
breakdown; and the migration carries the existing 246 rows over without losing
recorded time.

Context, recorded so the ledger is not read as a plan to weaken an oracle:

- Before this change `tasks` was one row per `(user_id, work_date, label)`, so
  creating the same label on a new day duplicated the row and its description.
  `task_comments` and `task_integrations` FK'd to that row, so a JIRA key was
  written once per day worked.
- The real database (`apps/web/local.db`) holds 246 task rows, 149 distinct
  `(user_id, label)` identities, and 804410 seconds of recorded time. The
  migration's job is to turn 246 rows into 149 identities plus 246 day rows
  while preserving that exact second count — measured from the DB, not copied
  from this sentence.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=767d47c7c35da852707024e32a5ac55ccfab36502df814bc5f3bb5db7634e522; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: the migration folds day rows into identities without losing recorded time
  CHECK: node scripts/verify-day-identity.mjs --migration
  EXPECT: migration verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=319fb0970b372934310839f91540d9a4ca1a81887977fc18b0e148346c0c037d; exit=0; EXPECT=matched; output-sha256=b81c4ce1865a696e293dacdd93c60ceef810ed7d49410bb75063565ba463d709; output-bytes=80; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the migration oracle rejects a migration that drops day rows
  CHECK: node scripts/verify-day-identity.mjs --migration --self-test
  EXPECT: migration control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=1476683fcf7133da48cde4b0c97c1db76aa8b650c774fa2d351101caf3476ede; exit=0; EXPECT=matched; output-sha256=cc13fc5d24ef61017d26bdcee05228c1d61067aa9fe51ffe56e0498832704664; output-bytes=104; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: a task worked on two days is one task with two timed days
  CHECK: pnpm --filter sv-task-timer exec vitest run --testNamePattern="is the same task with its own day time"
  EXPECT: /Tests\s+1 passed/
  EVIDENCE: automatic-evidence=v1; definition-sha256=0abf7e00b3d6c06b09f172e91f071c88ddc62d4199b2f51ea1799396a9571b52; exit=0; EXPECT=matched; output-sha256=7fb0485d5bf75c6a341bd80eca5da43ff11183c3d65461474e3224c306126264; output-bytes=237; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the archive returns one entry per task carrying every day worked
  CHECK: pnpm --filter sv-task-timer exec vitest run --testNamePattern="returns one entry per task carrying each day worked"
  EXPECT: /Tests\s+1 passed/
  EVIDENCE: automatic-evidence=v1; definition-sha256=cd935654768585895653fc73826da2072c3f799e0d96eb3d0dd0ab2172320320; exit=0; EXPECT=matched; output-sha256=0b3071e6d1e9baa83917ec71bf2fe1479496dedbe52eace61df1a24cb589945c; output-bytes=237; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: a JIRA key is stored once per task, not once per day worked
  CHECK: node scripts/verify-day-identity.mjs --integrations
  EXPECT: integration verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=69534b6835bc0bb44ea5b22367158306405266ee7dd45ba5f3c775ce3d8a8b33; exit=0; EXPECT=matched; output-sha256=ab1112e664906f62bb3239dcd1d3981be269faa09a002bb5b4ffe47c1f76969b; output-bytes=84; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G6: the web service and REST layer pass their tests
  CHECK: pnpm --filter sv-task-timer test
  EXPECT: /Tests\s+\d+ passed/
  EVIDENCE: automatic-evidence=v1; definition-sha256=72ca79258ccc512417a2d2fe3fd093c8940530f7047be41f66d6c99e723c8592; exit=0; EXPECT=matched; output-sha256=29b01379264d09a69da8cfeee3072e260fffa33cef6b4695ec6688838b5c001b; output-bytes=308; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G7: the TUI crate builds and every unit test passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=df46334085c96606b5cf109e4c386d3f2781b317476e11e1f4183a2e90791dff; output-bytes=5889; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G8: clippy reports no warnings on the TUI crate, all targets and features
  CHECK: node scripts/verify-clippy-clean.mjs
  EXPECT: clippy clean
  EVIDENCE: automatic-evidence=v1; definition-sha256=1e642a637ef1184752b89991d2181311373d43f334833c07e2498d9bbd739f65; exit=0; EXPECT=matched; output-sha256=f31ef79c253ce8c2fa97bf24a678530213886724c78c00b05d5b6acde6f94c28; output-bytes=13; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G9: the web app type-checks with no errors
  CHECK: pnpm --filter sv-task-timer check
  EXPECT: svelte-check found 0 errors
  EVIDENCE: automatic-evidence=v1; definition-sha256=761cf9c1a9c3c7866fa973983c2a61eb5ba2d8330e50616ce77462f0d41a03b6; exit=0; EXPECT=matched; output-sha256=cd88cea0892015315ed65c81add3b0ea57ed10e117943e478461959242bfb01b; output-bytes=6020; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G10: the TUI's timers still drive status and done through the day row
  CHECK: node scripts/verify-status-sync.mjs
  EXPECT: status sync verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=a8d3442788316b10f11b6d33d326ffd2629421cb1598be2a650c422ac821d5b4; exit=0; EXPECT=matched; output-sha256=ba70af49c2171c253b8ffdc91be101b4934c586424360713fbe38e8b66ce1b51; output-bytes=32; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G11: the TUI's status picker still writes the chosen catalog id
  CHECK: node scripts/verify-status-picker.mjs
  EXPECT: status picker verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=b14d9caa2655dfc2efe3e1ec792b750330bfefa921891a775cb292e5ad5c58b7; exit=0; EXPECT=matched; output-sha256=027bd8cdacdb3d6eafed5210eb1a5bf8643ee0714448ea3d01fae30733db470b; output-bytes=205; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G12: a TUI stop still worklogs and returns the issue to To Do
  CHECK: node scripts/verify-jira-stop.mjs
  EXPECT: stop verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=d6d9615d9e208ff3a21a658bcaa3d943ee8ce37eb3ea5cae4e0da00f2308c560; exit=0; EXPECT=matched; output-sha256=1dc7fbe6a89bb902c5772f7cebb24911f3141898becfd330ff0dd19aa63d4054; output-bytes=196; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G13: a keyed task still opens its JIRA issue in the browser
  CHECK: node scripts/verify-jira-open.mjs
  EXPECT: jira open verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=c7cb9b804338f7dd340d4279b9fbd9a7cebc04a6a423499a3d5fa5e1e18204e4; exit=0; EXPECT=matched; output-sha256=8f0a7f850a40b494641e902e436c42eeab89add88667f903da3401cc0e477fa4; output-bytes=201; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

<!--
G1 drives the real migrator over a copy of `apps/web/local.db` and measures the
resulting identity count, day-row count, and summed `task_days.elapsed_time`
independently from the source DB, then requires the elapsed total to match
exactly and every day row to point at a surviving identity.

G2 is the negative control: it feeds the same checker a copy whose day rows were
deleted first, and requires the predicate to reject it. Without that, a green G1
could mean the checker never looked.

G3/G4 name single cases in `apps/web/src/lib/server/taskDayIdentity.test.ts`,
which drives the real service against a migrated throwaway DB — so the
assertions read what the app returns rather than what the SQL was meant to do.
The name filter is deliberate: it fails if that exact case stops existing, which
a whole-suite `Tests N passed` would not notice.

G5 checks the migrated copy for duplicated or orphaned task-level rows.

G10–G13 drive the real binary in a pty. They must run against a freshly built
binary (`cargo build -p task-timer-tui`); a stale `target/debug` binary silently
tests the previous schema and fails with `no such column: status`.

G6/G7/G9 are the two projects' own regression suites. G8 mirrors the crate's
existing clippy gate.
-->

- [x] G14: renaming onto a label the user already has answers 409, not 500
  CHECK: pnpm --filter sv-task-timer exec vitest run --testNamePattern="409, not 500"
  EXPECT: /Tests\s+1 passed/
  EVIDENCE: automatic-evidence=v1; definition-sha256=529162e32fc716645ee8bd936071ac5174f854a29fef7aa1cb8210b9c19fc692; exit=0; EXPECT=matched; output-sha256=0526162d3a51923d61ff55cffb6c08150c2e4e7f341e14d1d6c2b41c0d173ff2; output-bytes=238; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G15: the archive honours the archived day filter
  CHECK: pnpm --filter sv-task-timer exec vitest run --testNamePattern="archived selects which days appear"
  EXPECT: /Tests\s+1 passed/
  EVIDENCE: automatic-evidence=v1; definition-sha256=aee0f848a25f3654c38c8f88555667bddf25094a67082a708b6fa72fb50778dd; exit=0; EXPECT=matched; output-sha256=9ace167004101de0390b7bb2d058d69a526f94c599724d0dd520629f32ee7818; output-bytes=237; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries
