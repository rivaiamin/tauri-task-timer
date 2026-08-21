import type { Handle } from '@sveltejs/kit';
import {
  SESSION_COOKIE,
  validateSessionToken,
  setSessionCookie,
  deleteSessionCookie
} from '$lib/server/auth/session';

export const handle: Handle = async ({ event, resolve }) => {
  const token = event.cookies.get(SESSION_COOKIE);

  if (!token) {
    event.locals.user = null;
    event.locals.session = null;
    return resolve(event);
  }

  const { session, user } = validateSessionToken(token);
  if (session && user) {
    // Refresh the cookie's expiry to match the (possibly slid) session.
    setSessionCookie(event.cookies, token, session.expiresAt, event.url.protocol === 'https:');
    event.locals.user = user;
    event.locals.session = session;
  } else {
    deleteSessionCookie(event.cookies);
    event.locals.user = null;
    event.locals.session = null;
  }

  return resolve(event);
};
