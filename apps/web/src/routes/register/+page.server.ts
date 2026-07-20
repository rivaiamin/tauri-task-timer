import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { z } from 'zod';
import { createUser } from '$lib/server/auth/user';
import { createSession, setSessionCookie } from '$lib/server/auth/session';

export const load: PageServerLoad = async ({ locals }) => {
  if (locals.user) throw redirect(302, '/dashboard');
};

const schema = z
  .object({
    email: z.string().trim().email(),
    password: z.string().min(6, 'Password must be at least 6 characters long'),
    confirmPassword: z.string()
  })
  .refine((v) => v.password === v.confirmPassword, {
    message: 'Passwords do not match',
    path: ['confirmPassword']
  });

export const actions: Actions = {
  default: async ({ request, cookies }) => {
    const form = Object.fromEntries(await request.formData());
    const parsed = schema.safeParse(form);
    if (!parsed.success) {
      return fail(400, {
        error: parsed.error.issues[0]?.message ?? 'Please check your details.',
        email: String(form.email ?? '')
      });
    }

    let user;
    try {
      user = await createUser(parsed.data.email, parsed.data.password);
    } catch (err) {
      if (err instanceof Error && err.message === 'EMAIL_TAKEN') {
        return fail(400, { error: 'An account with that email already exists.', email: parsed.data.email });
      }
      console.error('Registration failed:', err);
      return fail(500, { error: 'Failed to create account. Please try again.', email: parsed.data.email });
    }

    const { token, expiresAt } = createSession(user.id);
    setSessionCookie(cookies, token, expiresAt);
    throw redirect(302, '/dashboard');
  }
};
