# Tasks: Orchestrator Driver Conversation

Plan: [plan.md](plan.md). Scope: Rust TUI with a bundled Node adapter over Paseo's public SDK; runtime requires Node. Status: Tasks 1–5 implemented (Task 5 gates in `GATES-orchestrator-task5.md`); Task 6 and Checkpoint C remain. Use this file as the feature task tracker; leave the existing `docs/tui/tasks.md` backlog intact.

## Before Implementation

- [x] Amin approves the plan and its Node/SDK integration amendment.
- [x] Task 1 contract and adapter route accepted; read-only evidence is in [orchestrator-contract.md](../docs/tui/orchestrator-contract.md).

## Task 1: Prove the Paseo history and delivery contract

**Description:** Establish the smallest supported interface that can read the existing persistent driver's retained conversation and deliver a message to an exact agent. Installed `paseo logs --json` emitted plain timeline text in the planning probe. Inspect installed CLI/API documentation and implementation as needed, then verify read-only responses. Record actual schemas, roles, event IDs/order, older-history traversal, partial-output behavior, and errors. Do not send or start anything during this investigation. No application code is written in this task.

**Acceptance criteria:**
- [x] Document a verified supported history source that includes retained human prompts and assistant replies, provides older entries and deterministic ordering, and identifies automated wake prompts wherever the source retains them; explicitly document retention/partial-output limits. If no supported source meets this, block Task 3 and request an upstream/interface decision instead of weakening the feature.
- [x] Document label discovery, exact-target `paseo send --no-wait --json <id> --prompt <text>`, start/adopt behavior, and acknowledgment/error ambiguity from source/read-only evidence. Identify any assumptions that still require a separately authorized live send.
- [x] Freeze the adapter data contract and local-daemon approach around the approved bundled Node helper and public SDK. The runtime requires Node; no separate service, new database, or private-store access is introduced. Human approved this amendment before implementation.

**Verification:**
- [x] Read-only CLI/help/source and the selected public SDK history reads were performed; a redacted structural example includes an older-history read. The actual `logs --json` text output is documented.
- [x] No driver creation, message send, runner resume, timer mutation, or Jira transition occurred during contract verification.

**Dependencies:** None; plan and Node/SDK integration amendment approved before implementation.

**Files likely touched:**
- `docs/tui/orchestrator-contract.md` (new; verified contract and evidence)

**Estimated scope:** Small: 1 file.

## Task 2: Open a responsive read-only driver view

**Description:** Add `AppMode::Orchestrator` and the `O` entry point from non-modal Daily/Archive/Kanban. Discover labeled drivers and show identity/status without creating anything. Introduce the small command worker and explicit mode handling needed to keep render/input, timer updates, and hook handling responsive. Multiple driver matches require selection; absent/unreachable drivers are visible states. At this slice, the working path is read-only connection/status; history and send arrive in the next slices.

**Acceptance criteria:**
- [x] `O` opens the view, shows the full selected ID and verbatim driver status, and returns to the original mode without losing selection/date; zero or multiple matches are handled without implicit creation or arbitrary targeting.
- [x] Discovery runs off the UI thread with bounded requests, duration, and captured output; exit reaps owned processes/workers and ignores stale identity/generation responses. Delayed/unavailable commands do not freeze input, timer updates, or hooks.
- [x] Opening/refreshing is read-only; Orchestrator mode does not fall through to `reload_archive`, task mutation keys, or existing Jira/Bitbucket actions. Missing executables/daemon failures show persistent actionable errors, not a fabricated empty success.

**Verification:**
- [x] Focused behavior tests: `cargo test -p task-timer-tui orchestrator` for absent/ambiguous drivers, timeout/cleanup, and stale results, using hermetic fake commands. These tests are added by this task.
- [x] Build: `cargo build -p task-timer-tui --bin task-timer-tui`.
- [x] Actual TUI smoke: `target/debug/task-timer-tui --config <temporary-config>` with a throwaway DB/HOME and fake discovery that delays or fails. Open/exit from all three modes; observe timer/input responsiveness and no external mutations. This task does not use the installed production TUI.

**Progress evidence (Task 2 complete; final ledger verification pending):**

- The focused Orchestrator suite (9 tests), worker-cancellation regression, and Daily↔Archive key-dispatch regression passed in their respective runs. `cargo build -p task-timer-tui --bin task-timer-tui` succeeded.
- In an isolated temporary database/config, `O` displayed the fake driver's full ID and `idle` status from Daily, Archive, and Kanban; exit returned to the originating mode without losing the selected context. The multiple-driver list required explicit selection; zero drivers and a missing Paseo executable showed distinct visible states. No real driver or production timer database was used.
- A fake `paseo ls` delayed by 3 seconds left input/rendering responsive; the throwaway timer advanced from 00:00:15 to 00:00:40 while Orchestrator was open and was then stopped (`is_running=0`). Opening/history requests used only fake commands; no driver was started.

**Dependencies:** Task 1.

**Files likely touched:**
- `apps/tui/src/orchestrator.rs` (new adapter/worker with inline behavior tests)
- `apps/tui/src/main.rs` (register module)
- `apps/tui/src/app/mod.rs` (mode, state, input, loop integration)
- `apps/tui/src/ui/orchestrator_view.rs` (new renderer)
- `apps/tui/src/ui/mod.rs` (render dispatch)

**Estimated scope:** Medium: 5 files.

## Checkpoint A: Contract and read-only access

- [x] Task 1 contract approved, including history viability and unresolved send semantics.
- [x] Tasks 1–2 complete; focused tests and debug build pass.
- [x] Real TUI opens the intended driver view without starting a driver/resuming the runner, remains responsive under delayed discovery, and returns cleanly to the originating mode.
- [x] Human review: Amin approved the current identity-selection and disconnected/error presentation.

## Task 3: Browse retained conversation and new replies

**Description:** Connect the proven history interface to the view. Show true message roles and retained human/assistant content, load older entries, and refresh new replies/wakes. Extract Orchestrator-specific state/input into `app/orchestrator.rs` as the feature grows instead of expanding the already-large main app module. Paseo remains authoritative; loaded pages and transient view state are in memory only.

**Acceptance criteria:**
- [x] Reopening the view reads retained history for the selected driver, including human prompts, assistant replies, and recorded wake prompts; older-history loading reaches beyond the initial window and exposes retention boundaries. Equal-text messages remain distinct; ordering follows the proved transport contract.
- [x] Scrolling, older-history loading, Unicode wrapping, and narrow-terminal rendering work; refresh preserves a user's reading position and follows output only at the bottom. Render roles honestly and remove terminal control sequences; tool/thought content never becomes a human/assistant message by guesswork.
- [x] Refresh runs only while the view is active, at most one request at a time; identity changes discard stale results and separate transcripts. Read failures preserve visibly stale last-good history, with no duplicate pages or false completed-response state.

**Verification:**
- [x] Focused behavior tests: `cargo test -p task-timer-tui orchestrator` for real-schema normalization, repeated equal text, older/newer page boundaries, malformed payloads, control sequences, and stale-generation results. Avoid tests that assert source text or mere nonempty output.
- [x] Build: `cargo build -p task-timer-tui --bin task-timer-tui`.
- [x] Actual TUI smoke with throwaway configuration and deterministic history source: load older entries, scroll away from the bottom, append a response/wake, resize, disconnect/reconnect, and reopen; observe stable ordering and reading position. Read-only live comparison with the selected driver's supported history source is permitted; no live message send.

**Progress evidence:**

- Focused Orchestrator run: 15 tests passed. Regression cases cover duplicate sequence pages, wrapped Unicode scroll compensation, before/after cursor retention, oldest-history boundaries, stale/gapped pages, terminal controls in role/timestamp text, and stale-generation responses.
- Actual narrow TUI smoke loaded older entries, showed the retention-boundary warning through a later tail refresh, appended a wake/reply while scrolled away from bottom without moving the visible anchor, and displayed the new reply after scrolling down. The same temporary fixture demonstrated disconnected/stale history, reconnect, and retained history after reopening.
- Live PTY resize from 60×16 to 90×24 reflowed the transcript without losing the Orchestrator view. All CLI/SDK responses came from the fake Paseo/Node fixtures; no live message was sent.

**Dependencies:** Tasks 1–2; Checkpoint A.

**Files likely touched:**
- `apps/tui/src/orchestrator.rs`
- `apps/tui/src/app/orchestrator.rs` (new feature state/input module)
- `apps/tui/src/app/mod.rs`
- `apps/tui/src/ui/orchestrator_view.rs`

**Estimated scope:** Medium: 4 files.

## Task 4: Send one instruction to the selected driver

**Description:** Add a multiline composer that sends to the displayed persistent driver using the verified exact-target Paseo command. Do not route through the current `orchestrator-run driver-send` fallback. Show delivery state separately from agent activity and history; retain the draft on failure or uncertain delivery, with no automatic resend. Sending a message never implicitly creates a driver or resumes the queue.

**Acceptance criteria:**
- [x] `Tab` focuses composer/history; Unicode input and bracketed paste preserve message content; `Enter` adds a newline and `Ctrl+S` submits once. Empty input does not send; quotes, leading dashes, shell syntax, and embedded newlines are passed as literal argv data. Composer text cannot trigger timer/navigation/Jira actions.
- [x] The exact visible agent receives one submission while duplicate submit is disabled in flight. Transport acknowledgment shows accepted, not replied/completed; pre-dispatch refusal shows failed and nonzero/timeout/malformed outcomes show delivery-unknown according to the verified contract. Draft is preserved on uncertain delivery, no automatic retry or target fallback occurs, and retry requires an explicit duplicate-risk confirmation.
- [x] Subsequent authoritative history shows the human message, assistant reply, and an external-client message; no text-based deduplication or synthetic reply is added. Draft survives mode changes; quitting with a draft or pending send warns before discard. Drafts are in-memory only.

**Verification:**
- [x] Focused behavior tests: `cargo test -p task-timer-tui orchestrator` (22 passed) covers literal multiline/leading-dash argv, in-flight duplicate suppression, acceptance versus history, ambiguous failure and explicit retry, target disappearance, input precedence, and draft retention.
- [x] Build: `cargo build -p task-timer-tui --bin task-timer-tui`.
- [x] Actual TUI smoke against a throwaway database/config and fake Paseo/Node processes: bracketed multiline Unicode paste, exact-target literal argv, malformed acknowledgment and timeout after the fake process recorded the send, visible accepted/unknown states, preserved draft, quit warning, and no automatic second timeout submission (count remained one). Fake authoritative history showed retained/new replies; no live driver send or production DB was used.

**Progress evidence:**

- The focused Orchestrator suite passed 22 tests. Hermetic process tests verified the exact `send --no-wait --json visible-driver --prompt <literal multiline>` argv, one attempt while sending, accepted acknowledgment without synthesized history, authoritative history including an external-client entry, unknown delivery after nonzero exit, explicit retry confirmation, and stale-target handling.
- The actual TUI accepted bracketed paste containing Unicode, quotes, shell metacharacters, and newlines. A fake CLI timeout after recording argv displayed `Delivery unknown`, retained/locked the draft, and produced exactly one recorded attempt. Ctrl+C displayed a quit/discard confirmation; the TUI exited cleanly after confirmation. No production driver, runner, Jira, or timer mutation was used.

The `jira.rs` duplicate-test-attribute warning is pre-existing and unrelated. The `poll_detail_updates` dead-code warning was NOT unrelated: it came from this feature deleting that call from the run loop. Found in review and restored, with regression tests for paste-into-forms and history polling while composing.

**Dependencies:** Task 3.

**Files likely touched:**
- `apps/tui/src/orchestrator.rs`
- `apps/tui/src/app/orchestrator.rs`
- `apps/tui/src/app/mod.rs`
- `apps/tui/src/ui/orchestrator_view.rs`

**Estimated scope:** Medium: 4 files.

## Checkpoint B: Conversation round trip

- [x] Tasks 3–4 complete; focused tests and debug build pass.
- [x] Actual TUI demonstrates retained history, older entries/new replies (Task 3 smoke), multiline submission, and honest failure/unknown-delivery presentation without automatic resends (Task 4 smoke and process tests).
- [x] No transcript database or orchestrator state-file writes were introduced; opening/reconnecting and sending use the existing SDK/CLI interfaces only.
- [ ] Human reviews the conversation flow. Record any authorized live send separately from hermetic smoke evidence; a skipped live check is not a claimed pass.

## Task 5: Explicitly start or adopt a missing driver

**Description:** Provide the disconnected view with an explicit Start/adopt action backed by the existing `orchestrator-run driver-start`. Explain the bootstrap side effect and require confirmation. After the command returns an ID, rediscover/validate the driver and use the existing history/send path. Never implement a second driver lifecycle manager or automatically start the daemon.

**Acceptance criteria:**
- [x] With no driver discovered, the view offers Start/adopt with confirmation; cancel does nothing. Refresh/open/reconnect never starts it. Confirmed action invokes the existing idempotent wrapper once, shows progress, and adopts the returned validated driver rather than constructing a new `paseo run` command.
- [x] Repeated action while pending is disabled; malformed IDs, CLI failure, missing daemon, and identity races show explicit errors without follow-up send or retry. An already-existing driver is adopted, and ambiguous results require selection.
- [x] Start/adopt does not issue runner resume/tick, task start/stop, or Jira transitions; existing driver bootstrap and approval rules remain unchanged. Driver replacement is visible and does not merge histories or silently retarget an uncertain send.

**Verification:**
- [x] Focused behavior tests: `cargo test -p task-timer-tui orchestrator` for cancel, adoption, double activation, failure, malformed identity, and rediscovery races using fake lifecycle commands.
- [x] Build: `cargo build -p task-timer-tui`.
- [ ] Actual TUI smoke with isolated HOME and fake lifecycle: absent driver, cancel, explicit adoption, delayed creation, failure, then history/send availability. Do not create or replace the real persistent driver during automated verification.

**Dependencies:** Task 4; Checkpoint B.

**Files likely touched:**
- `apps/tui/src/orchestrator.rs`
- `apps/tui/src/app/orchestrator.rs`
- `apps/tui/src/ui/orchestrator_view.rs`

**Estimated scope:** Medium: 3 files.

## Task 6: Finish operator help and safety regression coverage

**Description:** After behavior is proven, document the actual Orchestrator keys, retained-history boundaries, side effects, and delivery meanings. Fill only remaining consumer-visible safety/compatibility regression gaps; do not add another framework or a source-string verification script. Verify the whole TUI still behaves normally outside the new mode.

**Acceptance criteria:**
- [ ] `?` help and `apps/tui/README.md` document entry/exit, focus, newline/send, history loading, explicit Start/adopt, missing dependencies, permission-attention handling, delivery-unknown recovery, and the difference between driver activity and runner/CI/ticket status.
- [ ] Regression checks cover composer-versus-global keys, out-of-mode timer/Jira/Archive/Kanban behavior, no mutation on read, stale target handling, draft/quit behavior, and subprocess cleanup under failures. Keep existing unrelated checks intact; web/MCP/database APIs remain unchanged.
- [ ] Complete the actual keyboard-driven TUI flow with a temporary configuration, including slow CLI, Unicode, resize, exit/reentry, delayed reply, and child cleanup; record operator evidence and any authorized live round trip without claiming unrun checks.

**Verification:**
- [ ] Focused tests: `cargo test -p task-timer-tui orchestrator`.
- [ ] Final suite/build checkpoint: `cargo test -p task-timer-tui` and `pnpm build:tui`.
- [ ] Manual actual-TUI check: use the newly built binary and throwaway DB/config; observe the scenario above and verify normal timer/Archive/Kanban navigation afterward. Docs must match observed keys and states, not this plan if implementation decisions changed.

**Dependencies:** Task 5.

**Files likely touched:**
- `apps/tui/src/app/keymap.rs`
- `apps/tui/README.md`
- `apps/tui/src/app/orchestrator.rs`
- `apps/tui/src/orchestrator.rs`
- `apps/tui/src/ui/orchestrator_view.rs`

**Estimated scope:** Medium: 5 files.

## Checkpoint C: Complete

- [ ] All six tasks and their acceptance criteria are complete; all contract gates are resolved without reducing history/messaging scope.
- [ ] Rust tests pass, release TUI builds, and actual TUI smoke evidence covers both the changed path and existing modes.
- [ ] Missing/unavailable dependencies fail visibly; no live driver/Jira mutations were performed by automated tests, no orphaned CLI processes remain, and no private transcripts were copied into repository docs.
- [ ] Operator documentation is updated; no scaffolds, throwaway files, placeholder history, fake responses, or unrelated changes remain.
- [ ] Amin reviews the finished feature and signs off before shipping. A live production-driver round trip requires separate exact-target/message authorization; record whether performed or still unverified.

## Implementation Scheduling

Run Tasks 1–6 in order. Checkpoints occur after every two tasks. These slices share UI and transport state, so default to one implementation owner rather than parallel edits. If later parallelized after Task 1, freeze the contract first and assign disjoint adapter/rendering files; integration and live verification remain sequential.
