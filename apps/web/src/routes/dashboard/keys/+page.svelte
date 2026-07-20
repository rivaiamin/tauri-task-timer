<script lang="ts">
  import type { PageData } from './$types';

  export let data: PageData;

  interface KeyRow {
    id: string;
    keyPrefix: string;
    name: string | null;
    revoked: boolean;
    lastUsedAt: string | Date | null;
    createdAt: string | Date | null;
  }

  let keys: KeyRow[] = data.keys as KeyRow[];
  let newName = '';
  let creating = false;
  let error = '';
  let createdKey: string | null = null; // raw key, shown once
  let copied = false;

  function fmt(d: string | Date | null): string {
    if (!d) return '—';
    const date = new Date(d);
    return isNaN(date.getTime()) ? '—' : date.toLocaleString();
  }

  async function createKey() {
    error = '';
    creating = true;
    try {
      const res = await fetch('/api/keys', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ name: newName.trim() || undefined })
      });
      if (!res.ok) {
        const e = await res.json().catch(() => ({}));
        throw new Error(e.message ?? 'Failed to create key');
      }
      const created = await res.json();
      createdKey = created.key;
      copied = false;
      keys = [
        {
          id: created.id,
          keyPrefix: created.keyPrefix,
          name: created.name,
          revoked: false,
          lastUsedAt: null,
          createdAt: new Date()
        },
        ...keys
      ];
      newName = '';
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to create key';
    } finally {
      creating = false;
    }
  }

  async function copyCreated() {
    if (!createdKey) return;
    try {
      await navigator.clipboard.writeText(createdKey);
      copied = true;
    } catch {
      copied = false;
    }
  }

  async function revoke(key: KeyRow) {
    if (key.revoked) return;
    if (!confirm(`Revoke key ${key.keyPrefix}…? Agents using it will stop working.`)) return;
    try {
      const res = await fetch(`/api/keys/${key.id}`, { method: 'DELETE' });
      if (!res.ok && res.status !== 204) {
        const e = await res.json().catch(() => ({}));
        throw new Error(e.message ?? 'Failed to revoke key');
      }
      keys = keys.map((k) => (k.id === key.id ? { ...k, revoked: true } : k));
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to revoke key');
    }
  }
</script>

<div class="min-h-screen p-4 sm:p-8 bg-gray-50 text-gray-900 dark:bg-gray-950 dark:text-gray-100">
  <div class="max-w-2xl mx-auto bg-white dark:bg-gray-900 rounded-xl shadow-lg p-6 sm:p-8 ring-1 ring-black/5 dark:ring-white/10">
    <header class="mb-6">
      <a href="/dashboard" class="text-sm font-semibold text-blue-600 dark:text-blue-400 hover:text-blue-700 dark:hover:text-blue-300">&larr; Back to dashboard</a>
      <h1 class="text-3xl font-bold text-gray-900 dark:text-gray-100 mt-2">API Keys</h1>
      <p class="text-gray-500 dark:text-gray-400 mt-1">
        Keys let AI agents and scripts control your timer via the API. Send one as
        <code class="text-xs bg-gray-100 dark:bg-gray-800 px-1 py-0.5 rounded">Authorization: Bearer &lt;key&gt;</code>.
      </p>
    </header>

    {#if createdKey}
      <div class="mb-6 p-4 rounded-lg border-2 border-amber-300 dark:border-amber-900/60 bg-amber-50 dark:bg-amber-950/20">
        <p class="text-sm font-semibold text-amber-800 dark:text-amber-300 mb-2">
          Copy your new key now — it won't be shown again.
        </p>
        <div class="flex items-center gap-2">
          <code class="flex-1 break-all text-sm font-mono bg-white dark:bg-gray-900 border border-amber-300 dark:border-amber-900/60 rounded px-2 py-2 text-gray-900 dark:text-gray-100">{createdKey}</code>
          <button on:click={copyCreated} type="button" class="shrink-0 px-3 py-2 text-sm font-semibold text-white bg-amber-600 rounded-lg hover:bg-amber-700 transition-colors">
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
        <button on:click={() => (createdKey = null)} type="button" class="mt-2 text-xs text-amber-700 dark:text-amber-400 hover:underline">Dismiss</button>
      </div>
    {/if}

    <form on:submit|preventDefault={createKey} class="flex flex-col sm:flex-row gap-2 mb-8">
      <input
        type="text"
        bind:value={newName}
        maxlength="100"
        placeholder="Key name (optional, e.g. claude-agent)"
        class="flex-1 p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400"
        disabled={creating}
      />
      <button type="submit" disabled={creating} class="p-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200 shadow-sm disabled:opacity-50 disabled:cursor-not-allowed">
        {creating ? 'Creating…' : 'Create key'}
      </button>
    </form>

    {#if error}
      <p class="mb-4 text-sm text-red-700 dark:text-red-400">{error}</p>
    {/if}

    <div class="space-y-3">
      {#if keys.length === 0}
        <p class="text-gray-500 dark:text-gray-400 text-center">No API keys yet. Create one above.</p>
      {:else}
        {#each keys as key (key.id)}
          <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 p-4 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/70 {key.revoked ? 'opacity-60' : ''}">
            <div class="min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="font-medium text-gray-900 dark:text-gray-100">{key.name || 'Unnamed key'}</span>
                <code class="text-xs font-mono text-gray-500 dark:text-gray-400">{key.keyPrefix}…</code>
                {#if key.revoked}
                  <span class="text-xs font-semibold text-red-700 dark:text-red-400 bg-red-100 dark:bg-red-950/40 px-1.5 py-0.5 rounded">Revoked</span>
                {/if}
              </div>
              <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                Created {fmt(key.createdAt)} · Last used {fmt(key.lastUsedAt)}
              </p>
            </div>
            {#if !key.revoked}
              <button on:click={() => revoke(key)} type="button" class="shrink-0 px-3 py-2 text-sm font-semibold text-white bg-red-500 hover:bg-red-600 rounded-lg transition-colors">
                Revoke
              </button>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
