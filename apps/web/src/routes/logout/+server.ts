import { redirect, type RequestHandler } from '@sveltejs/kit';
import { invalidateSession, deleteSessionCookie } from '$lib/server/auth/session';

export const POST: RequestHandler = async ({ locals, cookies }) => {
  if (locals.session) invalidateSession(locals.session.id);
  deleteSessionCookie(cookies);
  throw redirect(302, '/login');
};
