<script lang="ts">
  import { tick } from 'svelte';
  import { menuKey, menuPosition, moveFocus, typeahead } from '../lib/desktop/contextMenu';
  import { useMenu } from './menuController.svelte';
  import type { FileType } from '../lib/desktop/fileTypes';
  import FileIcon from './FileIcon.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  const menu = useMenu();
  let el: HTMLDivElement | undefined = $state();
  let pos = $state<{ x: number; y: number } | null>(null);
  let active = $state(-1);

  const m = $derived(menu.current);

  // Measure once drawn, then place it so it fits on screen; focus the menu (or its first item).
  $effect(() => {
    const open = m;
    if (!open || !el) return;
    pos = null;
    active = open.fromKeyboard ? moveFocus(open.entries, -1, 'first') : -1;
    void tick().then(() => {
      if (!el || menu.current !== open) return;
      const r = el.getBoundingClientRect();
      pos = menuPosition(
        open.at,
        { w: r.width, h: r.height },
        { width: window.innerWidth, height: window.innerHeight },
      );
      focusActive();
    });
  });

  function focusActive() {
    const target =
      active >= 0 ? el?.querySelector<HTMLElement>(`[data-index="${active}"]`) : (el ?? null);
    target?.focus({ preventScroll: true });
  }

  function choose(i: number) {
    const e = m?.entries[i];
    if (!e || e.kind !== 'item' || e.disabledReason) return;
    menu.choose(e.id);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!m) return;
    const action = menuKey(e.key);
    if (action === 'close') {
      e.preventDefault();
      menu.close(true);
    } else if (action === 'activate') {
      e.preventDefault();
      choose(active);
    } else if (action) {
      e.preventDefault();
      active = moveFocus(m.entries, active, action);
      focusActive();
    } else if (e.key.length === 1 && /\S/.test(e.key) && !e.ctrlKey && !e.metaKey) {
      active = typeahead(m.entries, active, e.key);
      focusActive();
    }
  }

  function onWindowPointer(e: PointerEvent) {
    if (m && el && !el.contains(e.target as Node)) menu.close(false);
  }
</script>

<svelte:window
  onpointerdowncapture={onWindowPointer}
  onresize={() => menu.close(false)}
  onblur={() => menu.close(false)}
/>

{#if m}
  <div
    bind:this={el}
    class="menu"
    class:placed={pos !== null}
    role="menu"
    aria-label={m.label}
    tabindex="-1"
    style:left="{pos?.x ?? m.at.x}px"
    style:top="{pos?.y ?? m.at.y}px"
    {onkeydown}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#if m.header}
      {@const h = m.header}
      <div class="header" aria-hidden="true">
        {#if h.icon}
          <span class="header-icon">
            {#if h.file}<FileIcon type={h.icon as FileType} size={28} />{:else}<Icon
                name={h.icon as IconName}
                size={18}
              />{/if}
          </span>
        {/if}
        <span class="header-text">
          <span class="header-title">{h.title}</span>
          {#if h.detail}<span class="header-detail">{h.detail}</span>{/if}
        </span>
      </div>
    {/if}
    {#each m.entries as e, i (i)}
      {#if e.kind === 'separator'}
        <div class="sep" role="separator"></div>
      {:else}
        <div
          class="item"
          class:danger={e.danger}
          class:disabled={!!e.disabledReason}
          role={e.checked === undefined ? 'menuitem' : 'menuitemradio'}
          tabindex="-1"
          data-index={i}
          aria-disabled={e.disabledReason ? 'true' : undefined}
          aria-keyshortcuts={e.shortcut}
          aria-checked={e.checked === undefined ? undefined : e.checked}
          aria-describedby={e.disabledReason ? `menu-reason-${i}` : undefined}
          onclick={() => choose(i)}
          onkeydown={() => {}}
          onpointerenter={() => {
            active = i;
            focusActive();
          }}
        >
          <span class="glyph" aria-hidden="true">
            {#if e.checked}<Icon name="check" size={16} />{:else if e.icon}<Icon
                name={e.icon as IconName}
                size={16}
              />{/if}
          </span>
          <span class="label">{e.label}</span>
          {#if e.shortcut}<kbd class="key" aria-hidden="true">{e.shortcut}</kbd>{/if}
          {#if e.disabledReason}
            <span class="reason" id="menu-reason-{i}" aria-hidden="true">{e.disabledReason}</span>
          {/if}
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 200000;
    min-width: 220px;
    max-width: min(320px, calc(100vw - 8px));
    max-height: calc(100vh - 8px);
    overflow-y: auto;
    padding: 6px;
    border-radius: var(--radius-lg);
    background: var(--menu);
    box-shadow: var(--shadow-pop);
    outline: none;
    visibility: hidden;
  }
  .menu.placed {
    visibility: visible;
    animation: appear var(--dur-fast) var(--ease-out);
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: scale(0.98);
    }
  }
  .header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-3);
    margin: 0 0 4px;
    border-bottom: 1px solid var(--line);
  }
  .header-icon {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: var(--radius-md);
    background: var(--surface-3);
    color: var(--text-muted);
  }
  .header-text {
    display: grid;
    min-width: 0;
  }
  .header-title {
    font-size: var(--text-sm);
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .header-detail {
    font-size: var(--text-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item {
    display: grid;
    grid-template-columns: 18px 1fr auto;
    align-items: center;
    column-gap: var(--space-3);
    min-height: 36px;
    padding: 4px var(--space-3) 4px var(--space-2);
    border-radius: var(--radius-md);
    font-size: var(--text-md);
    cursor: default;
    outline: none;
  }
  @media (pointer: coarse) {
    .item {
      min-height: var(--touch);
    }
  }
  .glyph {
    display: inline-flex;
    color: var(--text-muted);
  }
  .item:focus {
    background: var(--accent-soft);
  }
  .item:focus .glyph {
    color: var(--accent-ink);
  }
  .item:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .item.danger,
  .item.danger .glyph {
    color: var(--danger);
  }
  .item.danger:focus {
    background: var(--danger-soft);
  }
  .item.disabled,
  .item.disabled .glyph {
    color: var(--text-faint);
  }
  .item.disabled:focus {
    background: var(--hover);
    color: var(--text-muted);
  }
  .key {
    font-family: var(--font-ui);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .reason {
    grid-column: 2 / -1;
    font-size: var(--text-xs);
    line-height: 1.35;
    color: var(--text-muted);
  }
  .sep {
    height: 1px;
    margin: 5px var(--space-2);
    background: var(--line);
  }
</style>
