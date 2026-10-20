# Gates: Orchestrator driver conversation — contract

OWNS: docs/tui/orchestrator-contract.md, tasks/plan.md, tasks/todo.md

Scope: verify supported read-only history and delivery interfaces, then freeze a contract for human acceptance before transport-dependent implementation.

- [x] G1: supported Paseo history and delivery behavior are recorded from source and read-only evidence
  EVIDENCE: docs/tui/orchestrator-contract.md records installed CLI/SDK/protocol source and the read-only structured tail/older-page probe; no live send/start was issued.

- [x] G2: Amin explicitly accepts the recorded adapter contract before implementation proceeds
  EVIDENCE: On 2026-10-19, Amin selected the Node adapter using the public SDK, accepting the explicit Node/runtime and SDK dependency requirement; implementation is authorized on that basis.
