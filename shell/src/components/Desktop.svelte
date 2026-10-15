<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { useDesktop } from './desktopController.svelte';
  import ErrorNotice from './ErrorNotice.svelte';
  import Taskbar from './Taskbar.svelte';
  import type { WindowDef } from './windowDefs';
  import WindowFrame from './WindowFrame.svelte';

  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const byKind = $derived(new Map(defs.map((d) => [d.kind, d])));
</script>

<div class="desktop">
  <div class="wall" aria-hidden="true">
    <span class="mark">faber</span>
  </div>

  <main class="windows" aria-label="Desktop" style:height="{desktop.area.height}px">
    {#each desktop.state.windows as win (win.id)}
      {@const def = byKind.get(win.kind)}
      {#if def}
        {@const Content = def.component}
        <WindowFrame {win} icon={def.icon}>
          <Content {win} visible={!win.minimised} />
        </WindowFrame>
      {/if}
    {/each}
  </main>

  {#if core.lastError}
    <div class="toast">
      <ErrorNotice error={core.lastError}>
        {#snippet actions()}
          <button class="btn quiet" onclick={() => (core.lastError = null)}>Dismiss</button>
        {/snippet}
      </ErrorNotice>
    </div>
  {/if}

  <Taskbar {defs} />
</div>

<style>
  .desktop {
    position: relative;
    height: 100%;
    overflow: hidden;
  }
  /* A forge at dusk: steel-grey air, with the glow of hot iron along the bottom edge. */
  .wall {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(120% 60% at 30% 118%, var(--wall-glow), transparent 62%),
      linear-gradient(180deg, var(--wall-top), var(--wall-bottom));
  }
  .mark {
    position: absolute;
    right: 4vw;
    bottom: calc(var(--taskbar-h) + 3vh);
    font-size: clamp(5rem, 14vw, 13rem);
    font-weight: 780;
    letter-spacing: -0.06em;
    line-height: 0.8;
    color: var(--wall-mark);
  }
  .windows {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
  }
  .toast {
    position: absolute;
    right: var(--space-3);
    bottom: calc(var(--taskbar-h) + var(--space-3));
    width: min(420px, calc(100vw - 24px));
    z-index: 99998;
    border-radius: var(--radius-md);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
</style>
