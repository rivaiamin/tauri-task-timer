# Gates: Orchestrator Task 5 — explicit Start/adopt

OWNS: GATES-orchestrator-task5.md, apps/tui/src/orchestrator.rs, apps/tui/src/app/orchestrator.rs, apps/tui/src/ui/orchestrator_view.rs, apps/tui/README.md

Scope: when discovery finds no labeled driver, the Orchestrator view offers a confirmed Start/adopt action that invokes `orchestrator-run driver-start` once, validates the returned agent id, rediscovers drivers, and reuses the existing history/send path without auto-start on refresh.

- [x] G0: this ledger's declared checks are lint-valid
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES-orchestrator-task5.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=e60e06b9f05a0158d616932152db25794929afc05ab776827ed505cb53a5827c; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: orchestrator Rust tests cover Start/adopt cancel, adoption, failures, and malformed identity
  CHECK: cargo test -p task-timer-tui orchestrator
  EXPECT: /driver_start/
  EVIDENCE: automatic-evidence=v1; definition-sha256=cf6772c77673051345bc045c61665fbbaf6e1c4e75a1e5923270ba3a9b5154df; exit=0; EXPECT=matched; output-sha256=6bffc680a8a160f5bddaba97de2ebdb5bd3877fbfcdb28c203189650ced9821d; output-bytes=3633; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the TUI debug build succeeds after Task 5
  CHECK: cargo build -p task-timer-tui
  EXPECT: /Finished `dev` profile/
  EVIDENCE: automatic-evidence=v1; definition-sha256=be3b465c06cd726153bff0b3996a1305861e708b9c5486e45a89e95512ad589c; exit=0; EXPECT=matched; output-sha256=3dbebdfe57c58e155ac8a657a434094c1504a64a2cee500df8a6e14e483660d2; output-bytes=72; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: operator README documents Start/adopt confirmation and disallows silent refresh start
  CHECK: node scripts/verify-orchestrator-smoke.mjs --docs
  EXPECT: orchestrator docs verified
  EVIDENCE: automatic-evidence=v1; definition-sha256=95aac67a500cd388bacbcea65a66ffa189ff88db6437f95d4d4bea3847c94be6; exit=0; EXPECT=matched; output-sha256=1c7018baf3b28c3041947c66f507a4d921a977067cbf3a180da9e77c247ddc31; output-bytes=27; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries
