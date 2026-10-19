<script lang="ts">
  import { columnForJiraStatus } from "$lib/kanbanColumns";
  import { withPending } from "$lib/stores/pending";
  import type { PageData } from "./$types";
  export let data: PageData;
  let cards = data.cards;
  let statusMsg = "";
  let busy = false;
  async function reload() {
    const res = await fetch(`/api/kanban/cards?date=${data.workDate}`);
    if (res.ok) cards = (await res.json()).cards;
  }
  async function post(path: string, body?: object) {
    busy = true;
    statusMsg = "";
    try {
      await withPending(async () => {
        const res = await fetch(path, {
          method: "POST",
          headers: body ? { "content-type": "application/json" } : {},
          body: body ? JSON.stringify(body) : undefined
        });
        statusMsg = res.ok ? "ok" : await res.text();
        await reload();
      });
    } finally {
      busy = false;
    }
  }
</script>
<div class="p-4">
  <h1 class="text-xl font-semibold">Kanban</h1>
  <p class="text-sm text-gray-500">{data.projects.join(", ")} · {data.workDate}</p>
  <div class="flex gap-2 my-3 flex-wrap">
    <button disabled={busy} on:click={() => post("/api/kanban/refresh")}>Refresh</button>
    <button disabled={busy} on:click={() => post("/api/kanban/pull", { workDate: data.workDate })}>Pull</button>
    <button disabled={busy} on:click={() => post("/api/kanban/pipeline", { pipeline: "local-ok", dryRun: true })}>local-ok</button>
    <a href="/dashboard" class="underline">Daily</a>
  </div>
  {#if busy}<p class="text-sm text-indigo-600 dark:text-indigo-400">Working…</p>{/if}
  {#if statusMsg}<p class="text-sm">{statusMsg}</p>{/if}
  <div class="flex gap-2 overflow-x-auto">
    {#each data.columns as col}
      <div class="min-w-[12rem] border p-2 rounded">
        <h2 class="font-medium text-sm">{col}</h2>
        {#each cards.filter((c) => columnForJiraStatus(c.jiraStatus) === col) as card}
          <div class="text-xs border mt-1 p-1"><b>{card.issueKey}</b> {card.label}</div>
        {/each}
      </div>
    {/each}
  </div>
</div>
