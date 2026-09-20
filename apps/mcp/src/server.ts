import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod';

export type ApiFn = (path: string, method?: string, body?: unknown) => Promise<unknown>;

export function makeApi(baseUrl: string, apiKey: string): ApiFn {
  const base = baseUrl.replace(/\/$/, '');
  return async (path, method = 'GET', body) => {
    const res = await fetch(`${base}/api${path}`, {
      method,
      headers: {
        authorization: `Bearer ${apiKey}`,
        ...(body !== undefined ? { 'content-type': 'application/json' } : {})
      },
      body: body !== undefined ? JSON.stringify(body) : undefined
    });
    const text = await res.text();
    if (!res.ok) {
      let message = text;
      try {
        message = JSON.parse(text).message ?? text;
      } catch {
        /* non-JSON */
      }
      throw new Error(`${res.status} ${res.statusText}: ${message}`);
    }
    // JSON endpoints -> parse; text endpoints (e.g. /report) -> raw string.
    const contentType = res.headers.get('content-type') ?? '';
    if (contentType.includes('application/json')) return text ? JSON.parse(text) : null;
    return text || null;
  };
}

/** Build an MCP server whose tools call the Task Timer API with the given key. */
export function createServer(baseUrl: string, apiKey: string): McpServer {
  const api = makeApi(baseUrl, apiKey);
  const server = new McpServer({ name: 'task-timer', version: '0.1.0' });

  function tool(
    name: string,
    config: { title: string; description: string; inputSchema?: z.ZodRawShape },
    run: (args: any) => Promise<unknown>
  ) {
    // Omit inputSchema for no-arg tools so a call with no arguments validates.
    const toolConfig = config.inputSchema
      ? { title: config.title, description: config.description, inputSchema: config.inputSchema }
      : { title: config.title, description: config.description };
    server.registerTool(name, toolConfig, async (args: any) => {
      try {
        const result = await run(args ?? {});
        const text = typeof result === 'string' ? result : JSON.stringify(result ?? { ok: true }, null, 2);
        return { content: [{ type: 'text' as const, text }] };
      } catch (e) {
        return {
          content: [{ type: 'text' as const, text: `Error: ${e instanceof Error ? e.message : String(e)}` }],
          isError: true
        };
      }
    });
  }

  tool(
    'list_tasks',
    {
      title: 'List tasks',
      description:
        'List tasks with elapsed time and total. With date (YYYY-MM-DD) the result is that day: the tasks worked that day, with that day\'s time. Without date it is the archive: every task, each carrying a days[] array of its per-day entries. Also filter with q (label substring), tag, status, done, archived.',
      inputSchema: {
        date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional().describe('Work date YYYY-MM-DD'),
        q: z.string().optional().describe('Label substring (case-insensitive)'),
        tag: z.string().optional().describe('Tag substring'),
        status: z.string().optional(),
        done: z.boolean().optional(),
        archived: z.boolean().optional(),
      },
    },
    ({ date, q, tag, status, done, archived }: { date?: string; q?: string; tag?: string; status?: string; done?: boolean; archived?: boolean }) => {
      const params = new URLSearchParams();
      if (date) params.set('date', date);
      if (q) params.set('q', q);
      if (tag) params.set('tag', tag);
      if (status) params.set('status', status);
      if (done !== undefined) params.set('done', String(done));
      if (archived !== undefined) params.set('archived', String(archived));
      const qs = params.toString();
      return api(qs ? `/tasks?${qs}` : '/tasks');
    }
  );

  tool(
    'add_comment',
    {
      title: 'Add task comment',
      description: 'Attach a comment or delivery reference to a task.',
      inputSchema: {
        task_id: z.number().int(),
        subject: z.string().nullable().optional(),
        summary: z.string().nullable().optional(),
        branch: z.string().nullable().optional(),
        pr: z.string().nullable().optional()
      }
    },
    ({ task_id, subject, summary, branch, pr }) =>
      api(`/tasks/${task_id}/comments`, 'POST', { subject, summary, branch, pr })
  );

  tool(
    'list_comments',
    {
      title: 'List task comments',
      description: 'List all comments attached to a task.',
      inputSchema: { task_id: z.number().int() }
    },
    ({ task_id }) => api(`/tasks/${task_id}/comments`)
  );

  tool(
    'upsert_integration',
    {
      title: 'Upsert integration',
      description: 'Create or update an integration field on a task (e.g. JIRA sprint, Bitbucket branch).',
      inputSchema: {
        task_id: z.number().int(),
        group: z.string().min(1).describe('Integration group (e.g. "jira", "bitbucket")'),
        field: z.string().min(1).describe('Field name (e.g. "sprint", "branch")'),
        value: z.string().nullable()
      }
    },
    ({ task_id, group, field, value }) =>
      api(`/tasks/${task_id}/integrations`, 'PATCH', { group, field, value })
  );

  tool(
    'list_integrations',
    {
      title: 'List task integrations',
      description: 'List all integration fields on a task.',
      inputSchema: { task_id: z.number().int() }
    },
    ({ task_id }) => api(`/tasks/${task_id}/integrations`)
  );

  tool(
    'create_task',
    {
      title: 'Create task',
      description:
        'Create a task, or return the existing task with the same label. A task is identified by its label, so creating the same label again is the same task; a new day adds that day to it.',
      inputSchema: {
        label: z.string().min(1).describe('Task name'),
        description: z.string().optional(),
        workDate: z
          .string()
          .regex(/^\d{4}-\d{2}-\d{2}$/)
          .optional()
          .describe('Day to create the task on, YYYY-MM-DD (default: today)'),
        code: z.string().nullable().optional(),
        link: z.string().url().nullable().optional(),
        status: z.string().optional(),
        notes: z.string().nullable().optional(),
        tags: z.array(z.string()).nullable().optional()
      }
    },
    ({ label, description, workDate, code, link, status, notes, tags }) =>
      api('/tasks', 'POST', { label, description, workDate, code, link, status, notes, tags })
  );

  tool(
    'start_timer',
    {
      title: 'Start timer',
      description:
        "Start a task's timer for a day (default today), adding that day to the task if it does not have one yet. Omit `exclusive` to use the user's timer mode (focus stops others; parallel does not).",
      inputSchema: {
        task_id: z.number().int(),
        exclusive: z.boolean().optional().describe('If true, stop all other running timers first'),
        workDate: z
          .string()
          .regex(/^\d{4}-\d{2}-\d{2}$/)
          .optional()
          .describe('Day to time, YYYY-MM-DD (default: today)')
      }
    },
    ({ task_id, exclusive, workDate }) => api(`/tasks/${task_id}/start`, 'POST', { exclusive, workDate })
  );

  tool(
    'stop_timer',
    {
      title: 'Stop timer',
      description: "Stop a task's timer, accumulating elapsed time for that day.",
      inputSchema: {
        task_id: z.number().int(),
        workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
      }
    },
    ({ task_id, workDate }) => api(`/tasks/${task_id}/stop`, 'POST', { workDate })
  );

  tool(
    'reset_task',
    {
      title: 'Reset task',
      description: "Reset one task's elapsed time to zero for a day (default today).",
      inputSchema: {
        task_id: z.number().int(),
        workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
      }
    },
    ({ task_id, workDate }) => api(`/tasks/${task_id}/reset`, 'POST', { workDate })
  );

  tool(
    'reset_all',
    {
      title: 'Reset all',
      description: 'Reset every task to zero for a day (default today).',
      inputSchema: {
        workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
      }
    },
    ({ workDate }) => api('/tasks/reset-all', 'POST', { workDate })
  );

  tool(
    'update_task',
    {
      title: 'Update task',
      description:
        "Update a task's label, description, elapsed time (seconds), and/or done flag. Label, description, code, link, notes and tags belong to the task itself; elapsed, status, done and the flags belong to the day named by workDate (default today). Marking done moves the linked JIRA issue to Cek di Local.",
      inputSchema: {
        task_id: z.number().int(),
        label: z.string().min(1).optional(),
        description: z.string().nullable().optional(),
        elapsed_seconds: z.number().int().min(0).optional(),
        done: z.boolean().optional(),
        workDate: z
          .string()
          .regex(/^\d{4}-\d{2}-\d{2}$/)
          .optional()
          .describe('Day the day-scoped fields apply to, YYYY-MM-DD (default: today)'),
        code: z.string().nullable().optional()
        ,link: z.string().url().nullable().optional()
        ,status: z.string().optional()
        ,notes: z.string().nullable().optional()
        ,tags: z.array(z.string()).nullable().optional()
        ,is_completed: z.boolean().optional()
        ,is_cancelled: z.boolean().optional()
        ,is_deleted: z.boolean().optional()
        ,is_archived: z.boolean().optional()
        ,is_pinned: z.boolean().optional()
        ,is_important: z.boolean().optional()
      }
    },
    ({ task_id, label, description, elapsed_seconds, done, workDate, code, link, status, notes, tags, is_completed, is_cancelled, is_deleted, is_archived, is_pinned, is_important }) => {
      const body: Record<string, unknown> = {};
      if (label !== undefined) body.label = label;
      if (description !== undefined) body.description = description;
      if (elapsed_seconds !== undefined) body.elapsed_seconds = elapsed_seconds;
      if (done !== undefined) body.done = done;
      if (workDate !== undefined) body.workDate = workDate;
      if (code !== undefined) body.code = code;
      if (link !== undefined) body.link = link;
      if (status !== undefined) body.status = status;
      if (notes !== undefined) body.notes = notes;
      if (tags !== undefined) body.tags = tags;
      if (is_completed !== undefined) body.is_completed = is_completed;
      if (is_cancelled !== undefined) body.is_cancelled = is_cancelled;
      if (is_deleted !== undefined) body.is_deleted = is_deleted;
      if (is_archived !== undefined) body.is_archived = is_archived;
      if (is_pinned !== undefined) body.is_pinned = is_pinned;
      if (is_important !== undefined) body.is_important = is_important;
      return api(`/tasks/${task_id}`, 'PATCH', body);
    }
  );

  tool(
    'delete_task',
    { title: 'Delete task', description: 'Delete a task.', inputSchema: { task_id: z.number().int() } },
    async ({ task_id }) => {
      await api(`/tasks/${task_id}`, 'DELETE');
      return { deleted: task_id };
    }
  );

  tool(
    'reorder_tasks',
    {
      title: 'Reorder tasks',
      description: 'Set task order by giving all task ids in the desired order.',
      inputSchema: { ids: z.array(z.number().int()).min(1) }
    },
    ({ ids }) => api('/tasks/reorder', 'POST', { ids })
  );

  tool('get_timer_mode', { title: 'Get timer mode', description: 'Get the current timer mode (focus | parallel).' }, () =>
    api('/settings/timer-mode')
  );

  tool(
    'set_timer_mode',
    { title: 'Set timer mode', description: 'Set the timer mode.', inputSchema: { mode: z.enum(['focus', 'parallel']) } },
    ({ mode }) => api('/settings/timer-mode', 'PUT', { timer_mode: mode })
  );

  tool(
    'get_report',
    {
      title: 'Get report',
      description: 'Get a Daily Report of tracked time as text. format: "markdown" (default) or "csv".',
      inputSchema: { format: z.enum(['markdown', 'csv']).optional() }
    },
    ({ format }) => api(`/report?format=${format ?? 'markdown'}`)
  );

  // E6 — Session lifecycle tools
  tool(
    'session_start',
    {
      title: 'Session start',
      description: "Start a timer for an agent session. Provide task_id to start that task's timer (exclusive/focus mode).",
      inputSchema: { task_id: z.number().int().positive() }
    },
    ({ task_id }) => api('/session/hook', 'POST', { event: 'session_start', task_id })
  );

  tool(
    'session_pause',
    {
      title: 'Session pause',
      description: 'Pause the currently running timer (e.g. while waiting for user input).'
    },
    () => api('/session/hook', 'POST', { event: 'waiting_user' })
  );

  tool(
    'session_end',
    {
      title: 'Session end',
      description: 'Stop all running timers for today (e.g. when the agent session ends).'
    },
    () => api('/session/hook', 'POST', { event: 'session_end' })
  );

  return server;
}
