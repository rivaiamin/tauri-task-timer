import type { PageServerLoad } from './$types';
import { listApiKeys } from '$lib/server/apiKeys';

export const load: PageServerLoad = async ({ locals }) => {
  const user = locals.user!; // guaranteed by dashboard/+layout.server.ts
  return { keys: listApiKeys(user.id) };
};
