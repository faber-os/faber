<script lang="ts">
  import { useCore } from '../lib/state/context';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import Wordmark from '../components/Wordmark.svelte';

  // Every window is given these (see windowDefs.ts); this one does not need them.
  let props: WindowProps = $props();

  const core = useCore();
  const host = typeof location === 'undefined' ? '' : location.host;
</script>

<div class="about">
  <div class="hero">
    <Wordmark size={40} os />
    <p class="tagline">
      Latin for <em>maker</em>. An open-source operating system for the machines you build.
    </p>
  </div>

  {#if core.systemError}
    <ErrorNotice error={core.systemError}>
      {#snippet actions()}
        <button class="btn" onclick={() => core.loadSystem()}>Try again</button>
      {/snippet}
    </ErrorNotice>
  {/if}

  <dl class="facts">
    <dt>Core</dt>
    <dd>
      {#if core.system}{core.system.name} <span class="num">{core.system.version}</span>{:else}Not
        known yet{/if}
    </dd>
    <dt>Shell</dt>
    <dd class="num">{SHELL_VERSION}</dd>
    <dt>Address</dt>
    <dd><code>{host}</code></dd>
    <dt>Connection</dt>
    <dd>{core.connection}, WebSocket as <code>{core.format}</code></dd>
  </dl>

  <p class="licence">Apache-2.0. Faber is in early development: this is shell v0.</p>
</div>

<style>
  .about {
    display: grid;
    align-content: start;
    gap: var(--space-5);
    flex: 1;
    overflow-y: auto;
    padding: var(--space-6) var(--space-5) var(--space-5);
    background: var(--surface);
    user-select: text;
  }
  .hero {
    display: grid;
    gap: var(--space-3);
  }
  .tagline {
    max-width: 40ch;
    color: var(--text-muted);
  }
  .tagline em {
    font-style: normal;
    font-weight: 650;
    color: var(--text);
  }
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-5);
    margin: 0;
    padding-top: var(--space-4);
    border-top: 1px solid var(--line);
    font-size: var(--text-sm);
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    font-weight: 560;
  }
  .licence {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
