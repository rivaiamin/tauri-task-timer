# Gates: Orchestrator driver conversation — implementation

OWNS: apps/tui/**, docs/tui/orchestrator-contract.md, tasks/plan.md, tasks/todo.md, package.json, pnpm-lock.yaml, scripts/verify-orchestrator-smoke.mjs

Scope: deliver the approved TUI driver conversation using a packaged Node adapter over the public Paseo SDK, with safe lifecycle, history, messaging, documentation, and responsive UI behavior.

- [x] G0: the implementation ledger's declared checks are lint-valid
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES-orchestrator-feature.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=e7455d8315b357d716dc35998d11f5f2362e1c962869bc1df16f759660d1c91f; exit=0; EXPECT=matched; output-sha256=4922104775cc129d01a1db150d4b2779d48d0e716925056c89bcef1762629d76; output-bytes=289; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: Orchestrator-specific Rust behavior tests pass
  CHECK: cargo test -p task-timer-tui orchestrator
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=b60110d9d9403f9da10fe2fed0aed48f53c911e08176deb3fcf2765376b38ad3; exit=0; EXPECT=matched; output-sha256=f19af21ffff3bc68b9890271fd9ddacf9fcc475be32561d91f8a2d4481398f67; output-bytes=3090; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the complete TUI Rust test suite passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=8c97246e2260b1df1a5e3a31c291e446b13ea23e92e2c2d984ae9e71e9dbd5f0; output-bytes=12082; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: the TUI release build succeeds
  CHECK: pnpm build:tui
  EXPECT: /Finished `release` profile/
  EVIDENCE: automatic-evidence=v1; definition-sha256=9540d156c95618c0c88e8b010d92ccb1d83ec93a89095aa3aab4b2363f4c6235; exit=0; EXPECT=matched; output-sha256=9a6c59a46b5f16619e5c75a8807620b5417abaf101a12c1042d0f4f1a03786f2; output-bytes=188; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: interactive TUI smoke proves the new flow, failure states, and existing modes against temporary configuration and a fake Paseo adapter
  CHECK: node scripts/verify-orchestrator-smoke.mjs
  EXPECT: orchestrator smoke verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=f6ec70e26b61c63e8a00f9a2516bdc15fb3e64477492221f90d6b07c519b1754; exit=0; EXPECT=matched; output-sha256=36e01e618653a03f2a48482b0383e895c7617e527fae7de0be7b069bec07dd9f; output-bytes=211; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: operator docs and the recorded transport contract match the implemented keys, delivery states, limitations, and Node/SDK runtime requirement
  CHECK: node scripts/verify-orchestrator-smoke.mjs --docs
  EXPECT: orchestrator docs verified
  EVIDENCE: automatic-evidence=v1; definition-sha256=95aac67a500cd388bacbcea65a66ffa189ff88db6437f95d4d4bea3847c94be6; exit=0; EXPECT=matched; output-sha256=1c7018baf3b28c3041947c66f507a4d921a977067cbf3a180da9e77c247ddc31; output-bytes=27; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries


[You have received this identical output 3 times. Re-reading '/home/amin/projects/tauri/tauri-task-timer/GATES-orchestrator-feature.md:raw' will not change it — use a narrower selector (path:A-B), or proceed with the edit.]