<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { signUp, getSession } from '$lib/auth/helpers';

  let email = '';
  let password = '';
  let confirmPassword = '';
  let error = '';
  let success = false;
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

    if (!email || !password || !confirmPassword) {
      error = 'Please fill in all fields';
      loading = false;
      return;
    }

    if (password !== confirmPassword) {
      error = 'Passwords do not match';
      loading = false;
      return;
    }

    if (password.length < 6) {
      error = 'Password must be at least 6 characters long';
      loading = false;
      return;
    }

    const { data, error: signUpError } = await signUp(email, password);

    if (signUpError) {
      error = signUpError.message || 'Failed to create account. Please try again.';
      loading = false;
      return;
    }

    if (data?.user) {
      // Check if email confirmation is required
      if (data.session) {
        // Auto-confirmed, redirect to dashboard
        goto('/dashboard');
      } else {
        // Email confirmation required
        success = true;
        loading = false;
      }
    } else {
      error = 'Registration failed. Please try again.';
      loading = false;
    }
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-blue-50 to-indigo-100 p-4">
  <div class="max-w-md w-full bg-white rounded-xl shadow-lg p-8">
    <div class="text-center mb-8">
      <h1 class="text-3xl font-bold text-gray-900 mb-2">Create Account</h1>
      <p class="text-gray-600">Sign up to start tracking your tasks</p>
    </div>

    {#if success}
      <div class="mb-4 p-4 bg-green-50 border border-green-200 rounded-lg">
        <p class="text-sm text-green-800 font-semibold mb-2">Account created successfully!</p>
        <p class="text-sm text-green-700">
          Please check your email to confirm your account before signing in.
        </p>
      </div>
      <div class="text-center">
        <a
          href="/login"
          class="inline-block px-4 py-2 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200"
        >
          Go to Sign In
        </a>
      </div>
    {:else}
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
            autocomplete="new-password"
            minlength="6"
            class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            placeholder="At least 6 characters"
            disabled={loading}
          />
          <p class="mt-1 text-xs text-gray-500">Must be at least 6 characters long</p>
        </div>

        <div>
          <label for="confirmPassword" class="block text-sm font-medium text-gray-700 mb-2">
            Confirm Password
          </label>
          <input
            type="password"
            id="confirmPassword"
            bind:value={confirmPassword}
            required
            autocomplete="new-password"
            class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            placeholder="Confirm your password"
            disabled={loading}
          />
        </div>

        <button
          type="submit"
          disabled={loading}
          class="w-full p-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200 shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {#if loading}
            Creating account...
          {:else}
            Sign Up
          {/if}
        </button>
      </form>

      <div class="mt-6 text-center">
        <p class="text-sm text-gray-600">
          Already have an account?{' '}
          <a href="/login" class="text-blue-600 hover:text-blue-700 font-semibold">
            Sign in
          </a>
        </p>
      </div>
    {/if}
  </div>
</div>

