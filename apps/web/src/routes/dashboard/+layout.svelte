<script lang="ts">
  import { onMount } from 'svelte';
  import { getSession } from '$lib/auth/helpers';
  import { goto } from '$app/navigation';

  let session: any = null;

  onMount(async () => {
    session = await getSession();
    if (!session) {
      goto('/login');
    }
  });
</script>

{#if session}
  <slot />
{:else}
  <div class="flex items-center justify-center min-h-screen">
    <p>Redirecting to login...</p>
  </div>
{/if}
