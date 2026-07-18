import { error, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { buildReport } from '$lib/server/taskService';

// GET /api/report?format=markdown|csv — a Daily Report over current elapsed times.
// Returns the raw report text with an appropriate content-type.
export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:read');

  const format = event.url.searchParams.get('format') ?? 'markdown';
  if (format !== 'markdown' && format !== 'csv') {
    throw error(400, 'format must be "markdown" or "csv"');
  }

  const dateISO = new Date().toISOString().slice(0, 10);
  const { contentType, body } = buildReport(actor.userId, format, dateISO);
  return new Response(body, { headers: { 'content-type': contentType } });
};
