# Orchestrator driver transport contract

Status: history transport is verified against the installed Paseo 0.11.0 daemon and public SDK; Amin approved a bundled Node adapter using the public `@getpaseo/client` SDK on 2026-10-19. Runtime requires Node.

## History

`paseo logs <id> --json` is not a structured JSON history API in the installed `@getpaseo/cli` 0.11.0. Its implementation fetches a projected timeline with `direction: "tail"`, `limit: 0`, and then prints `curateAgentActivity(...)` text directly. It has no cursor option for older pages. A read-only call with `--tail 6 --json` returned human-readable `[Thought]`, `[Shell]`, and assistant-text lines, not JSON. Do not parse that rendering as chat history.

The supported `@getpaseo/client` public SDK exposes `agents.ref(id).timeline.refetch({ direction, cursor, limit, projection })`. The response exposes `epoch`, `startCursor`, `endCursor`, `hasOlder`, `hasNewer`, and ordered `entries`; each entry has `item`, `timestamp`, `seqStart`, `seqEnd`, and `sourceSeqRanges`. A cursor is `{ epoch, seq }`. Projection choices are `projected` and `canonical`; the CLI uses projected.

Read-only SDK probe against the one labeled driver returned a five-entry tail page at sequence ranges 191–204 with `hasOlder: true`; a `before` request using the returned `startCursor` returned three older entries and `hasOlder: true`. A role-only scan of the currently available history found 5 `user_message`, 3 `assistant_message`, 28 `tool_call`, 22 `reasoning`, and 2 `notification` entries across three 20-entry-or-smaller pages; `hasOlder` became false at the oldest available page. No message text or transcript content was stored here. This verifies retained human/assistant roles and older-page traversal for the tested daemon, not indefinite retention or an exposed retention-limit reason.

Timeline roles are discriminated: `user_message`, `assistant_message`, `reasoning`, `tool_call`, `todo`, `error`, `notification`, `compaction`, and `plugin`. `messageId` is optional on user and assistant messages. Entry sequence ranges plus epoch provide page ordering/identity; repeated equal text must remain separate entries. Rendering must not relabel non-message events as human/assistant content.

The runner's `_wake_driver` sends its automated wake as a normal `paseo send --no-wait --json <id> <message>` prompt beginning `orchestrator wake:`. The SDK timeline type has no separate wake-origin field; when retained, it is a user-message entry identifiable by its message content, not structured provenance. No wake-prefixed entry appeared in the scanned history for the current driver, so that code path's retained representation is inferred from the send and timeline contracts, not directly observed. The SDK exposes `hasOlder`/cursors, but no global retention boundary metadata was observed. Partial streaming is a separate live-event surface; the history page contract exposes persisted timeline entries, not a completion guarantee for a currently streaming assistant turn.

## Discovery, send, and lifecycle

- `paseo ls -g --label orchestrator.role=driver --json` returned one active, non-archived labeled driver. CLI source confirms global listing and exact label filtering. Never use CLI ordering to silently choose among multiple IDs.
- `paseo inspect <id> --json` exposes the complete ID and current status; the live inspection reported `idle`. `idle` is activity status, not task completion.
- `paseo send --no-wait --json <id> --prompt <text>` targets the supplied ID. CLI source passes the prompt to `sendAgentMessage` and, with `--no-wait`, returns `status: "sent"` and “Message sent, not waiting for completion.” This acknowledges command acceptance only; it is not a reply or completion. A nonzero/timeout after dispatch may be ambiguous; the CLI source does not prove non-acceptance. Do not retry automatically. No live message was sent for this contract probe.
- `/home/amin/.agents/scripts/orchestrator_run.py` implements `driver_start()` by listing drivers and adopting the first result if any; otherwise it invokes `paseo run -d` with the bootstrap prompt. It writes orchestrator state and an event. This side effect requires an explicit user-confirmed lifecycle action. The wrapper does not disambiguate multiple drivers itself, so the TUI must not call it when discovery is ambiguous. Its `driver_send()` may fall back to another labeled ID and is unsuitable for exact-target composer sends.

## Integration decision

The Rust TUI can invoke the CLI, but the CLI history command only returns a rendered tail and cannot satisfy the structured older-history contract. The public SDK is TypeScript and is installed as a dependency of the global Paseo CLI, not as a task-timer project dependency. Amin approved adding it as a project build dependency and bundling the adapter into the Rust binary; execution will still require Node. Do not import the CLI's private nested module path or silently fall back to parsing rendered logs.

Evidence gathered read-only on 2026-10-19: `paseo ls -g --label orchestrator.role=driver --json`, `paseo inspect <id> --json`, `paseo logs --help`, `paseo inspect --help`, `paseo send --help`, `paseo logs <id> --tail 6 --json`, `paseo status --json`, installed CLI/SDK/protocol source, and a structured SDK tail plus `before` page probe. No driver was created, no message sent, and no runner/Jira/timer state changed.
