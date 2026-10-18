# Gates: Kanban board

OWNS: GATES-kanban.md, apps/tui/src/kanban.rs, apps/tui/src/ui/kanban_view.rs, apps/web/src/lib/server/kanban.ts, apps/web/src/lib/server/jiraIngest.ts, apps/web/src/lib/kanbanColumns.ts, apps/web/src/routes/api/kanban/**, apps/web/src/routes/dashboard/kanban/**, apps/mcp/src/server.ts, scripts/verify-kanban.mjs, scripts/verify-kanban-jira.mjs, scripts/verify-kanban-pipeline.mjs, docs/tui/epics.md

Scope: Jira-linked Kanban cards (non-Done) across TUI CLI, web `/dashboard/kanban` + `/api/kanban/*`, and MCP `kanban_*` tools; sprint pull, status refresh, and pipeline wrappers.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES-kanban.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=4949640992c2e3bd172a85468c9a572581717b06dc08a45e4d3ca0e2f6d373c9; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: TUI `kanban list` materializes linked non-Done cards from a throwaway DB
  CHECK: node scripts/verify-kanban.mjs
  EXPECT: kanban verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=34692aa93dba3816fe4867c86cfd99c100737c8b4989bf2364d1d3fb18b26767; exit=0; EXPECT=matched; output-sha256=f716384c427dcb0d98a907e942aca75c02614674d08c949d06a452113507cb18; output-bytes=198; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: web `listKanbanCards` returns linked non-Done rows
  CHECK: pnpm --filter sv-task-timer exec vitest run src/lib/server/kanban.test.ts
  EXPECT: /Tests\s+7 passed/
  EVIDENCE: automatic-evidence=v1; definition-sha256=132846679bb07d782ec393f83b0053cabcd827d01d809a6f12cbcab8cf22949c; exit=0; EXPECT=matched; output-sha256=8b472c38af6b51dc9770132fbc1415489e9ed87b85463ab123cace87fed9436e; output-bytes=199; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: web routes, MCP tools, and web kanban test are wired together
  CHECK: node scripts/verify-kanban.mjs --parity
  EXPECT: kanban parity verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=68f417667c58df20e9cc1b458cf329c8dbf1acab3a2426bf0529ee2f6294d289; exit=0; EXPECT=matched; output-sha256=38843dd07c6986d09054ea7451844389e9be6a3b6caf0654d0fd8782cac61a09; output-bytes=34; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the TUI crate builds and every unit test passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=82bf0606f0e29804c780df6c73b724e198e23e465a2360a890a259f3d24a5f41; output-bytes=9860; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: TUI `kanban pull` ingests issues from a stub Jira search (no network)
  CHECK: node scripts/verify-kanban-jira.mjs --pull
  EXPECT: kanban pull stub verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=76445a78cdcce4d411798d9b951200b96107b9085338b2f6adc5428b936769a3; exit=0; EXPECT=matched; output-sha256=e0870604e349f20434c468cead784ebc0e1f2a9af9fc2ac911805dc1aeb793ea; output-bytes=208; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G6: TUI `kanban refresh` updates `jira/status` from a stub issue fetch
  CHECK: node scripts/verify-kanban-jira.mjs --refresh
  EXPECT: kanban refresh stub verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=6aecd32bd625786471aaa2baed626c4540534d3ef72f1245357cdd44326dd93a; exit=0; EXPECT=matched; output-sha256=45662b006679db4b209f9d1d2af626cf360fc0862e4e168e19dedd96d8d8079a; output-bytes=211; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G7: TUI `kanban pipeline local-ok` runs a configured stub subprocess
  CHECK: node scripts/verify-kanban-pipeline.mjs
  EXPECT: kanban pipeline stub verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=e0696e0113f45acc3439ad3b1e05b179b6c5062d4f88c182936e1edf089ee5e5; exit=0; EXPECT=matched; output-sha256=ae1ae2fb4783ff66a87d8aac2495341a220ae349e6ab9cf7dfd1c9768033731a; output-bytes=212; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G8: web dashboard kanban page wires load + client refresh to the API
  CHECK: node scripts/verify-kanban.mjs --dashboard
  EXPECT: kanban dashboard smoke passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=e0f99300bea92ef838de30e6ab4b29ca2670f56d4cbb2ad99cd42359cf80e849; exit=0; EXPECT=matched; output-sha256=b83674fd5c02ebe48d1bde0387607fd4547d6cadf7a0afd5fd81e30f5505c574; output-bytes=30; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries
