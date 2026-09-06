import { type RequestHandler } from '@sveltejs/kit';
import { resolveActor } from '$lib/server/actor';
import { subscribe, type ChangePayload } from '$lib/server/events';

// GET /api/stream — Server-Sent Events: pushes a "change" whenever any of the
// caller's tasks are mutated (by this browser, another device, or an agent).
// Authenticated via the session cookie (EventSource sends cookies automatically).
export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  const encoder = new TextEncoder();

  const stream = new ReadableStream({
    start(controller) {
      const send = (payload: unknown) =>
        controller.enqueue(encoder.encode(`data: ${JSON.stringify(payload)}\n\n`));

      send({ type: 'connected' });
      const unsubscribe = subscribe(actor.userId, (change: ChangePayload) =>
        send({ type: 'change', ...change })
      );

      // Keep-alive comment so proxies don't drop an idle connection.
      const ping = setInterval(() => {
        try {
          controller.enqueue(encoder.encode(': ping\n\n'));
        } catch {
          // stream closed
        }
      }, 25000);

      event.request.signal.addEventListener('abort', () => {
        clearInterval(ping);
        unsubscribe();
        try {
          controller.close();
        } catch {
          // already closed
        }
      });
    }
  });

  return new Response(stream, {
    headers: {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
      connection: 'keep-alive'
    }
  });
};
