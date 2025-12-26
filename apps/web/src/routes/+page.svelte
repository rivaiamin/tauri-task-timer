<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getSession } from '$lib/auth/helpers';

  onMount(async () => {
    const session = await getSession();
    if (session) {
      goto('/dashboard');
    } else {
      goto('/login');
    }
  });
</script>

<div class="min-h-screen flex items-center justify-center">
  <p>Redirecting...</p>
</div>
