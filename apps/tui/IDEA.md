# TUI — Original brainstorm

> **Superseded by planning docs.** Requirements, epics, tasks, tech spec, and ERD
> now live in **[`docs/tui/`](../../docs/tui/README.md)**. This file is kept as
> historical context only.

## Quick links

- [PRD](../../docs/tui/prd.md)
- [Epics](../../docs/tui/epics.md)
- [Task checklist](../../docs/tui/tasks.md)
- [Tech spec](../../docs/tui/tech-spec.md)
- [ERD](../../docs/tui/erd.md)

---

## Original notes (2026)

It's a TUI for the task timer but more with advanced features. Goal of this app are to focus on this terminal to check the task and manage timer so it's easier for multitasking.

<details>
<summary>Full original spec (click to expand)</summary>

- Task
  - task can be unlinked or linked to third-party like JIRA
  - saved into databases with these field:
    - code, title, description, link, created_at, updated_at, status
    - start_time, end_time, total_time, notes, tags
    - is_running, is_completed, is_cancelled, is_deleted, is_archived, is_pinned, is_important
  - task_comments: discussion, comment, feedback (task_id, subject, summary, branch, PR)
  - task_integrations: third party data (task_id, group, field, value)
- Task timer page: per day, continue-by-title, counter resets daily
- Tasks archive page: non-finished tasks, filter/refetch
- Jira Integration: sprint sync, comment, status
- Git/bitbucket: commits, PR comments, deploy status
- AI Agent: CRUD + timer hooks (start, pause on wait, stop on session end)

</details>
