<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { signIn, getSession } from '$lib/auth/helpers';

  let email = '';
  let password = '';
  let error = '';
  let loading = false;

  onMount(async () => {
    // Redirect if already logged in
    const session = await getSession();
    if (session) {
      goto('/dashboard');
    }
  });

  async function handleSubmit(event: Event) {
    event.preventDefault();
    error = '';
    loading = true;

    if (!email || !password) {
      error = 'Please fill in all fields';
      loading = false;
      return;
    }

    const { data, error: signInError } = await signIn(email, password);

    if (signInError) {
      error = signInError.message || 'Failed to sign in. Please check your credentials.';
      loading = false;
      return;
    }

    if (data?.session) {
      // Successfully signed in, redirect to dashboard
      goto('/dashboard');
    } else {
      error = 'Sign in failed. Please try again.';
      loading = false;
    }
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-blue-50 to-indigo-100 p-4">
  <div class="max-w-md w-full bg-white rounded-xl shadow-lg p-8">
    <div class="text-center mb-8">
      <h1 class="text-3xl font-bold text-gray-900 mb-2">Task Timer</h1>
      <p class="text-gray-600">Sign in to your account</p>
    </div>

    {#if error}
      <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg">
        <p class="text-sm text-red-800">{error}</p>
      </div>
    {/if}

    <form on:submit={handleSubmit} class="space-y-6">
      <div>
        <label for="email" class="block text-sm font-medium text-gray-700 mb-2">
          Email
        </label>
        <input
          type="email"
          id="email"
          bind:value={email}
          required
          autocomplete="email"
          class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
          placeholder="you@example.com"
          disabled={loading}
        />
      </div>

      <div>
        <label for="password" class="block text-sm font-medium text-gray-700 mb-2">
          Password
        </label>
        <input
          type="password"
          id="password"
          bind:value={password}
          required
          autocomplete="current-password"
          class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
          placeholder="Enter your password"
          disabled={loading}
        />
      </div>

      <button
        type="submit"
        disabled={loading}
        class="w-full p-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200 shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {#if loading}
          Signing in...
        {:else}
          Sign In
        {/if}
      </button>
    </form>

    <div class="mt-6 text-center">
      <p class="text-sm text-gray-600">
        Don't have an account?{' '}
        <a href="/register" class="text-blue-600 hover:text-blue-700 font-semibold">
          Sign up
        </a>
      </p>
    </div>
  </div>
</div>

