# Task Timer TUI — Planning Docs

Terminal-first task timer and the **full product roadmap** for the monorepo. TUI leads; web dashboard and MCP must stay in sync so nothing is forgotten when schema or behavior changes.

## Documents

| Doc | Purpose |
|-----|---------|
| [prd.md](./prd.md) | Product requirements — problem, users, MVP, all future phases |
| [epics.md](./epics.md) | Epics with goals and acceptance criteria (TUI + web + MCP) |
| [tasks.md](./tasks.md) | Actionable checklist (done + backlog) per app |
| [e7-plan.md](./e7-plan.md) | E7 web dashboard parity — implementation plan |
| [tech-spec.md](./tech-spec.md) | Architecture, crate layout, API/MCP touchpoints, concurrency |
| [erd.md](./erd.md) | Current and target database entities |

## Phase map

```text
E1 Daily Timer MVP        ██████████  shipped
E2 Extended schema        ██████████  shipped
E3 Archive view           ██████████  shipped (TUI + API; web UI is E7)
E4 JIRA integration       ██████████  shipped
E5 Git / Bitbucket        ██████████  shipped
E6 AI agent hooks         ██████████  shipped
E7 Web dashboard parity   ███░░░░░░░  in progress — [e7-plan.md](./e7-plan.md)
```

## Affected apps per epic

| Epic | `apps/tui` | `apps/web` | `apps/mcp` | `packages/shared` |
|------|------------|------------|------------|---------------------|
| E1 | ✅ | schema migration | — | timer.ts (reference) |
| E2 | ✅ | schema, taskService, API | create/update + add_comment | types |
| E3 | ✅ | archive list API | list_tasks filters | — |
| E4 | ✅ | jira.ts | JIRA tools | — |
| E5 | ✅ | Bitbucket module | optional tools | — |
| E6 | ✅ | POST /api/session/hook | session tools | — |
| E7 | — | dashboard daily + archive UI | optional workDate on create | — |

## Related repo docs

- [apps/tui/README.md](../../apps/tui/README.md) — setup and keybindings
- [apps/tui/IDEA.md](../../apps/tui/IDEA.md) — original brainstorm → see [prd.md](./prd.md)
- [docs/api.md](../api.md) — REST API reference
- [docs/ai-control.md](../ai-control.md) — MCP architecture

## Source of truth

Requirements live in this folder. Implementation status: [tasks.md](./tasks.md). Mirror any Cursor plan changes here — plans are ephemeral, git is not.
