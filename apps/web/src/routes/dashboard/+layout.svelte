<script lang="ts">
  import { pendingCount } from '$lib/stores/pending';

  // Access is enforced server-side in +layout.server.ts.
</script>

{#if $pendingCount > 0}
  <div
    class="fixed top-0 left-0 right-0 z-50 h-0.5 bg-indigo-500 animate-pulse"
    role="status"
    aria-live="polite"
    aria-label="Loading"
  ></div>
{/if}

<slot />
