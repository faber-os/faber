<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { useDesktop } from './desktopController.svelte';
  import Icon from './Icon.svelte';
  import type { WindowDef } from './windowDefs';
  import Wordmark from './Wordmark.svelte';

  let { defs, onclose }: { defs: WindowDef[]; onclose: (returnFocus: boolean) => void } = $props();

  const core = useCore();
  const desktop = useDesktop();
  let menu: HTMLElement | undefined = $state();

  const GROUPS = [
    { id: 'parts', label: 'Parts' },
    { id: 'tools', label: 'Tools' },
    { id: 'faber', label: 'Faber' },
  ] as const;

  const viewers = $derived(
    (core.parts ?? []).flatMap((p) =>
      p.signals
        .filter((s) => s.capability === 'laser-scan@1')
        .map((s) => ({ part: p.name, address: s.address })),
    ),
  );

  $effect(() => {
    menu?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  });

  function items(): HTMLElement[] {
    return [...(menu?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
  }

  function onkeydown(e: KeyboardEvent) {
    const all = items();
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose(true);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      all[(i + 1) % all.length]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      all[(i - 1 + all.length) % all.length]?.focus();
    } else if (e.key === 'Home') {
      e.preventDefault();
      all[0]?.focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      all.at(-1)?.focus();
    } else if (e.key === 'Tab') {
      onclose(false);
    }
  }

  function open(kind: string) {
    desktop.open(kind);
    onclose(false);
  }

  function openViewer(v: { part: string; address: string }) {
    desktop.open('laser-scan', {
      key: v.address,
      title: `${v.part} · scan`,
      params: { address: v.address, part: v.part },
    });
    onclose(false);
  }
</script>

<div
  class="menu"
  id="start-menu"
  role="menu"
  aria-label="Start"
  tabindex="-1"
  bind:this={menu}
  {onkeydown}
>
  <div class="head">
    <Wordmark size={22} />
    <span class="tag">{core.system ? `v${core.system.version}` : ''}</span>
  </div>
  {#each GROUPS as g (g.id)}
    {@const list = defs.filter((d) => d.menu === g.id)}
    {#if list.length}
      <div class="group" role="group" aria-label={g.label}>
        <p class="group-label" aria-hidden="true">{g.label}</p>
        {#each list as d (d.kind)}
          <button class="item" role="menuitem" onclick={() => open(d.kind)}>
            <span class="icon"><Icon name={d.icon} size={20} /></span>
            <span class="text">
              <span class="name">{d.title}</span>
              <span class="desc">{d.description}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
  {/each}
  {#if viewers.length}
    <div class="group" role="group" aria-label="Viewers">
      <p class="group-label" aria-hidden="true">Viewers</p>
      {#each viewers as v (v.address)}
        <button class="item" role="menuitem" onclick={() => openViewer(v)}>
          <span class="icon"><Icon name="scan" size={20} /></span>
          <span class="text">
            <span class="name">{v.part}</span>
            <span class="desc mono">{v.address}</span>
          </span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu {
    position: absolute;
    left: 8px;
    bottom: calc(var(--taskbar-h) + 8px);
    width: min(360px, calc(100vw - 16px));
    max-height: calc(100vh - var(--taskbar-h) - 16px);
    overflow-y: auto;
    padding: var(--space-3) var(--space-2) var(--space-2);
    border-radius: var(--radius-lg);
    background: var(--menu);
    backdrop-filter: var(--chrome-blur);
    -webkit-backdrop-filter: var(--chrome-blur);
    box-shadow: var(--shadow-pop), var(--highlight);
    animation: rise var(--dur-med) var(--ease-out);
    z-index: 100000;
    outline: none;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: var(--space-2) var(--space-3) var(--space-3);
  }
  .tag {
    font-family: var(--font-data);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .group + .group {
    margin-top: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--line);
  }
  .group-label {
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
    font-weight: 650;
    color: var(--text-muted);
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 52px;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    text-align: left;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--hover);
  }
  .item:focus-visible {
    outline-offset: -2px;
  }
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--surface-3);
    color: var(--text-muted);
    flex: none;
  }
  .text {
    display: grid;
    min-width: 0;
  }
  .name {
    font-weight: 620;
    font-size: var(--text-md);
  }
  .desc {
    font-size: var(--text-sm);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
