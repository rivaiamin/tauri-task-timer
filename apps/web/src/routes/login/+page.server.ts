import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { z } from 'zod';
import { verifyUser } from '$lib/server/auth/user';
import { createSession, setSessionCookie } from '$lib/server/auth/session';

export const load: PageServerLoad = async ({ locals }) => {
  if (locals.user) throw redirect(302, '/dashboard');
};

const schema = z.object({
  email: z.string().trim().email(),
  password: z.string().min(1)
});

export const actions: Actions = {
  default: async ({ request, cookies }) => {
    const form = Object.fromEntries(await request.formData());
    const parsed = schema.safeParse(form);
    if (!parsed.success) {
      return fail(400, { error: 'Enter a valid email and password.', email: String(form.email ?? '') });
    }

    const user = await verifyUser(parsed.data.email, parsed.data.password);
    if (!user) {
      return fail(400, { error: 'Invalid email or password.', email: parsed.data.email });
    }

    const { token, expiresAt } = createSession(user.id);
    setSessionCookie(cookies, token, expiresAt);
    throw redirect(302, '/dashboard');
  }
};
