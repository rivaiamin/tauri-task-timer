import type { PageServerLoad } from './$types';
import { listArchive } from '$lib/server/taskService';

export const load: PageServerLoad = async ({ locals, url }) => {
  const user = locals.user!;
  const q = url.searchParams.get('q') || undefined;
  const tag = url.searchParams.get('tag') || undefined;
  const status = url.searchParams.get('status') || undefined;
  const doneParam = url.searchParams.get('done');
  const done = doneParam === 'true' ? true : doneParam === 'false' ? false : undefined;
  // Grouped archive: one entry per task identity, each with every day it was worked.
  const { tasks, totalElapsedSeconds } = listArchive(user.id, { q, tag, status, done });
  return { tasks, totalElapsedSeconds };
};
