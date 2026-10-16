<script lang="ts">
  import Icon from './Icon.svelte';

  /**
   * Why the controls beside it are disabled ("Needs the admin role"), so a role that can't use
   * something still sees it and knows whom to ask (docs/auth.md). Nothing when `reason` is null.
   */
  let { reason }: { reason: string | null } = $props();
</script>

{#if reason}
  <p class="lock"><Icon name="key" size={15} /><span>{reason}</span></p>
{/if}

<style>
  .lock {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
