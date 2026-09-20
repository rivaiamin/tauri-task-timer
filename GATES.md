# Gates: resolve the two open defects left after the status fix

OWNS: GATES.md, apps/tui/src/bitbucket.rs, scripts/verify-jira-stop.mjs, scripts/verify-clippy-clean.mjs

Scope: `cargo clippy -p task-timer-tui --all-targets --all-features -- -D warnings`
exits zero with no warning, `node scripts/verify-jira-stop.mjs --paths` passes and
additionally proves every stop path writes local status, and the status/JIRA
behavioral oracles that the previous commit touched still pass with their
negative controls. One of the two defects was introduced earlier in this branch;
the other is pre-existing on `main`. Both are described below rather than
assumed, because the difference decides who broke what.

Context for the two defects, recorded so the ledger is not read as a plan to
weaken an oracle:

- `bitbucket.rs:159` uses `filter_map` for a closure that always returns `Some`.
  `map` is the same behavior with less indirection. `commit_statuses` arrived in
  `76ba5bc` on this same branch, so `main` is clippy-clean and this branch broke
  it — the lint is this branch's own regression, fixed at its tip.
- `verify-jira-stop.mjs --paths` asserts a `jira::fire_on_stop` that commit
  `01fb464` deleted as dead code. That commit is already in `main`, whose
  `--paths` mode fails the same way, so this one predates the branch: the check
  had failed on every revision since, because no ledger ran that mode. Its
  replacement oracle `record_and_reopen` is already asserted, so the stale block
  is removed rather than resurrected — and the same check gains the assertion
  whose absence let the original bug ship: every stop path must write the task's
  local status, not only the JIRA issue.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node /home/amin/.agents/skills/unlazy/scripts/gate-lint.mjs GATES.md
  EXPECT: LINT OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=767d47c7c35da852707024e32a5ac55ccfab36502df814bc5f3bb5db7634e522; exit=0; EXPECT=matched; output-sha256=48630b7361dd44ee870917b12c3d19b9d7bdea738aaca16bb04d4cab83b772d2; output-bytes=8; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G1: clippy reports no warnings on the TUI crate, all targets and features
  CHECK: node scripts/verify-clippy-clean.mjs
  EXPECT: clippy clean
  EVIDENCE: automatic-evidence=v1; definition-sha256=1e642a637ef1184752b89991d2181311373d43f334833c07e2498d9bbd739f65; exit=0; EXPECT=matched; output-sha256=f31ef79c253ce8c2fa97bf24a678530213886724c78c00b05d5b6acde6f94c28; output-bytes=13; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G2: the clippy oracle rejects a warning transcript instead of always passing
  CHECK: node scripts/verify-clippy-clean.mjs --self-test
  EXPECT: clippy control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=efbff5d9383ea28ccb1980f30ebd8d1bd20c4c9db322fc03a23159edd71b409d; exit=0; EXPECT=matched; output-sha256=7d671eb1363637ef945dc9cdfb422b81b60e545ec76119d5c5267ecc8c2a84d1; output-bytes=22; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G3: every stop path routes through the JIRA stop oracle and writes local status
  CHECK: node scripts/verify-jira-stop.mjs --paths
  EXPECT: stop path coverage passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=df64638884b96555d2a6c1069a97821290f176cf4c330627c15578841a0d0247; exit=0; EXPECT=matched; output-sha256=84bb0d6defc13db5b085f018da643ede7d1c601c3f2a644c832075a5f6e082f7; output-bytes=26; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G4: the TUI crate builds and every unit test passes
  CHECK: cargo test -p task-timer-tui
  EXPECT: test result: ok
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e6c8eaa72a84af3d130ed3274aa97333a8d57496a1f1e5e9ab9131028d00c8d; exit=0; EXPECT=matched; output-sha256=1179921de0e7103d184bb7e2c211331abcaf427fe1bc0924ab6dd40464210af4; output-bytes=4894; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G5: the JIRA stop behavior and its negative control still hold
  CHECK: node scripts/verify-jira-stop.mjs
  EXPECT: stop verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=d6d9615d9e208ff3a21a658bcaa3d943ee8ce37eb3ea5cae4e0da00f2308c560; exit=0; EXPECT=matched; output-sha256=7fb04f2d08f6f0ce4e70cfcd3aec08efb7a420580e85c64553fb4810408b1eb7; output-bytes=196; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G6: the JIRA stop oracle's control still fails on a worklog-only stop
  CHECK: node scripts/verify-jira-stop.mjs --self-test
  EXPECT: stop oracle control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=6d012ec9901606222715a102394dcfb53f1301a56a554ee8a3778e47a262d321; exit=0; EXPECT=matched; output-sha256=5b03e20a8c3a719f746fadcc95623e367194663a5cd55d2179e7b57cb0222452; output-bytes=198; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G7: the status-sync oracle still holds, and its control still fails on pre-change behavior
  CHECK: node scripts/verify-status-sync.mjs --self-test
  EXPECT: status sync control passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=374f6d53d2934b218506ca68599b558a65d5d13aacfd0c688c3190f28b35c513; exit=0; EXPECT=matched; output-sha256=4f2ae8fe17e9c3f4cc7119ae823e505071108e71bac70b36ea8f342c66f6e4f7; output-bytes=27; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

- [x] G8: the status-picker oracle still holds through a real pty session
  CHECK: node scripts/verify-status-picker.mjs
  EXPECT: status picker verification passed
  EVIDENCE: automatic-evidence=v1; definition-sha256=b14d9caa2655dfc2efe3e1ec792b750330bfefa921891a775cb292e5ad5c58b7; exit=0; EXPECT=matched; output-sha256=ff170f6f59b11445989f2d641b52beba60a7ca96a75db4c3e35cc441e63eb511; output-bytes=205; shell=/bin/sh; cwd=/home/amin/projects/tauri/tauri-task-timer; path=0087377b4ce4/29 entries

<!--
G2 is the negative control for G1: the same assertion logic is fed a captured
clippy warning transcript and must reject it, so a green G1 means the detector
works rather than that it cannot fail. G6 and G7 are the pre-existing controls
for the two oracles the previous commit changed; they are re-run here because
that commit's diff is what G3's new assertion describes.

G3's `--paths` mode reads source text, so it cannot prove runtime behavior; it
proves only that each stop path still contains the calls. G5 and G8 observe
actual behavior against a stub API and a real pty respectively.
-->
