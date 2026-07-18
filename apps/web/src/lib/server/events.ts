// Tiny in-process pub/sub for live updates. Single-node only (fine for SQLite):
// task mutations publish() to a user's channel, and the SSE endpoint subscribes.
type Listener = () => void;

const channels = new Map<string, Set<Listener>>();

export function subscribe(userId: string, listener: Listener): () => void {
  let set = channels.get(userId);
  if (!set) {
    set = new Set();
    channels.set(userId, set);
  }
  set.add(listener);
  return () => {
    const s = channels.get(userId);
    if (!s) return;
    s.delete(listener);
    if (s.size === 0) channels.delete(userId);
  };
}

export function publish(userId: string): void {
  const set = channels.get(userId);
  if (!set) return;
  for (const listener of set) {
    try {
      listener();
    } catch {
      // ignore a broken subscriber
    }
  }
}
