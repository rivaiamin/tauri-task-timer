# Task Timer TUI — Planning Docs

Terminal-first task timer and the **full product roadmap** for the monorepo. TUI leads; web dashboard and MCP must stay in sync so nothing is forgotten when schema or behavior changes.

## Documents

| Doc | Purpose |
|-----|---------|
| [prd.md](./prd.md) | Product requirements — problem, users, MVP, all future phases |
| [epics.md](./epics.md) | Epics with goals and acceptance criteria (TUI + web + MCP) |
| [tasks.md](./tasks.md) | Actionable checklist (done + backlog) per app |
| [tech-spec.md](./tech-spec.md) | Architecture, crate layout, API/MCP touchpoints, concurrency |
| [erd.md](./erd.md) | Current and target database entities |

## Phase map

```text
E1 Daily Timer MVP        ██████████  shipped (feat/tui-mvp)
E2 Extended schema        ░░░░░░░░░░  planned (web + TUI + API)
E3 Archive view           ░░░░░░░░░░  planned (TUI)
E4 JIRA integration       ░░░░░░░░░░  planned (TUI + web taskService)
E5 Git / Bitbucket        ░░░░░░░░░░  planned
E6 AI agent hooks         ░░░░░░░░░░  planned (MCP + hooks + TUI listener)
E7 Web dashboard parity   ░░░░░░░░░░  planned (daily view, extended fields)
```

## Affected apps per epic

| Epic | `apps/tui` | `apps/web` | `apps/mcp` | `packages/shared` |
|------|------------|------------|------------|---------------------|
| E1 | ✅ | schema migration | — | timer.ts (reference) |
| E2 | db layer | schema, taskService, API | new tools if needed | types |
| E3 | archive UI | optional API filters | — | — |
| E4 | JIRA client | jira.ts / taskService | — | — |
| E5 | Bitbucket UI | taskService | — | — |
| E6 | hook listener | session endpoint? | session tools | — |
| E7 | — | dashboard daily view | — | types |

## Related repo docs

- [apps/tui/README.md](../../apps/tui/README.md) — setup and keybindings
- [apps/tui/IDEA.md](../../apps/tui/IDEA.md) — original brainstorm → see [prd.md](./prd.md)
- [docs/api.md](../api.md) — REST API reference
- [docs/ai-control.md](../ai-control.md) — MCP architecture (update when E6 ships)

## Source of truth

Requirements live in this folder. Implementation status: [tasks.md](./tasks.md). Mirror any Cursor plan changes here — plans are ephemeral, git is not.
