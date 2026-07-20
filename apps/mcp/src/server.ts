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

  tool('list_tasks', { title: 'List tasks', description: 'List all tasks with elapsed time and the total.' }, () =>
    api('/tasks')
  );

  tool(
    'create_task',
    {
      title: 'Create task',
      description: 'Create a new task.',
      inputSchema: { label: z.string().min(1).describe('Task name'), description: z.string().optional() }
    },
    ({ label, description }) => api('/tasks', 'POST', { label, description })
  );

  tool(
    'start_timer',
    {
      title: 'Start timer',
      description:
        "Start a task's timer. Omit `exclusive` to use the user's timer mode (focus stops others; parallel does not).",
      inputSchema: {
        task_id: z.number().int(),
        exclusive: z.boolean().optional().describe('If true, stop all other running timers first')
      }
    },
    ({ task_id, exclusive }) => api(`/tasks/${task_id}/start`, 'POST', { exclusive })
  );

  tool(
    'stop_timer',
    { title: 'Stop timer', description: "Stop a task's timer, accumulating elapsed time.", inputSchema: { task_id: z.number().int() } },
    ({ task_id }) => api(`/tasks/${task_id}/stop`, 'POST')
  );

  tool(
    'reset_task',
    { title: 'Reset task', description: "Reset a single task's elapsed time to zero.", inputSchema: { task_id: z.number().int() } },
    ({ task_id }) => api(`/tasks/${task_id}/reset`, 'POST')
  );

  tool('reset_all', { title: 'Reset all', description: 'Reset every task to zero.' }, () => api('/tasks/reset-all', 'POST'));

  tool(
    'update_task',
    {
      title: 'Update task',
      description: 'Update a task label, description, and/or elapsed time (seconds).',
      inputSchema: {
        task_id: z.number().int(),
        label: z.string().min(1).optional(),
        description: z.string().nullable().optional(),
        elapsed_seconds: z.number().int().min(0).optional()
      }
    },
    ({ task_id, label, description, elapsed_seconds }) => {
      const body: Record<string, unknown> = {};
      if (label !== undefined) body.label = label;
      if (description !== undefined) body.description = description;
      if (elapsed_seconds !== undefined) body.elapsed_seconds = elapsed_seconds;
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

  return server;
}
