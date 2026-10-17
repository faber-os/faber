<script lang="ts">
  /**
   * One level of a right-click menu (CircuitMenu keeps the stack of them). Arrow keys move, →
   * opens a submenu, ← and Esc close, Enter picks. Hovering an item that adds wires shows them
   * dashed on the canvas.
   */
  import { onMount, tick } from 'svelte';
  import { useCircuit } from './circuit.svelte';
  import type { MenuItem } from './menuItem';

  let {
    items,
    x,
    y,
    onclose,
    depth = 0,
    onback,
    onsub,
    openIndex,
    parentWidth = 0,
  }: {
    items: MenuItem[];
    x: number;
    y: number;
    onclose: () => void;
    depth?: number;
    onback?: () => void;
    /** Opens (or with `null`, closes) the submenu of item `index`. */
    onsub: (index: number | null, x: number, y: number) => void;
    /** The item whose submenu is open. */
    openIndex: number | null;
    parentWidth?: number;
  } = $props();

  const lab = useCircuit();
  let el: HTMLDivElement;
  let pos = $state<{ left: number; top: number } | null>(null);
  let active = $state(-1);

  export function focusAt(i: number) {
    focusItem(i);
  }

  onMount(async () => {
    await tick();
    const r = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let left = x;
    let top = y;
    if (left + r.width > vw - 8) left = depth ? x - r.width - parentWidth - 4 : vw - r.width - 8;
    if (top + r.height > vh - 8) top = Math.max(8, vh - r.height - 8);
    pos = { left: Math.max(8, left), top };
    focusItem(items.findIndex((i) => !i.disabled && !i.separator));
  });

  function focusItem(i: number) {
    active = i;
    const btn = el?.querySelectorAll<HTMLButtonElement>(':scope > .items > button')[i];
    btn?.focus();
  }

  function openSub(i: number, target: HTMLElement) {
    const item = items[i];
    if (!item?.children || item.disabled) {
      onsub(null, 0, 0);
      return;
    }
    const r = target.getBoundingClientRect();
    onsub(i, r.right + 2, r.top - 6);
  }

  function pick(i: number, target: HTMLElement) {
    const item = items[i]!;
    if (item.disabled) return;
    if (item.children) {
      openSub(i, target);
      return;
    }
    item.action?.();
    onclose();
  }

  function hover(i: number, target: HTMLElement) {
    active = i;
    const item = items[i]!;
    lab.preview = item.preview ?? null;
    if (item.children) openSub(i, target);
    else onsub(null, 0, 0);
  }

  function onkeydown(e: KeyboardEvent) {
    const usable = items
      .map((it, i) => (!it.separator && !it.disabled ? i : -1))
      .filter((i) => i >= 0);
    const at = usable.indexOf(active);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopPropagation();
      focusItem(usable[(at + 1) % usable.length]!);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopPropagation();
      focusItem(usable[(at - 1 + usable.length) % usable.length]!);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      e.stopPropagation();
      const btn = el.querySelectorAll<HTMLButtonElement>(':scope > .items > button')[active];
      if (btn && items[active]?.children) openSub(active, btn);
    } else if (e.key === 'ArrowLeft' && depth > 0) {
      e.preventDefault();
      e.stopPropagation();
      onback?.();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (depth > 0) onback?.();
      else onclose();
    }
  }
</script>

<div
  class="menu"
  bind:this={el}
  style:left="{pos?.left ?? x}px"
  style:top="{pos?.top ?? y}px"
  role="menu"
  tabindex="-1"
  {onkeydown}
>
  <div class="items">
    {#each items as item, i (i)}
      {#if item.separator}
        <hr />
      {:else}
        <button
          role="menuitem"
          class:danger={item.danger}
          class:on={openIndex === i}
          disabled={item.disabled}
          aria-haspopup={item.children ? 'menu' : undefined}
          onpointerenter={(e) => hover(i, e.currentTarget)}
          onfocus={() => (active = i)}
          onclick={(e) => pick(i, e.currentTarget)}
        >
          {#if item.swatch}<i class="swatch" style:background={item.swatch}></i>{/if}
          {#if item.mark}<i class="mark {item.mark}"></i>{/if}
          <span class="text">
            <span class="label">{item.label}</span>
            {#if item.hint}<span class="hint">{item.hint}</span>{/if}
          </span>
          {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
          {#if item.children}<span class="chev" aria-hidden="true">›</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    min-width: 220px;
    max-width: 340px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-pop);
    padding: 4px;
    font-size: 13px;
    outline: none;
  }
  .items {
    max-height: min(70vh, 520px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  button {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: none;
    text-align: left;
    padding: 6px 8px;
    border-radius: 5px;
    cursor: pointer;
    width: 100%;
    color: inherit;
  }
  button:hover:not(:disabled),
  button:focus-visible,
  button.on {
    background: var(--accent-soft);
    outline: none;
  }
  button:disabled {
    cursor: default;
    color: var(--text-faint);
  }
  .danger {
    color: var(--danger-text);
  }
  .text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .label {
    font-weight: 500;
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  button:disabled .hint {
    color: var(--text-faint);
  }
  kbd {
    font-family: var(--font-data);
    font-size: 11px;
    color: var(--text-muted);
  }
  .chev {
    color: var(--text-muted);
    font-size: 15px;
  }
  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    border: 1px solid var(--line-strong);
    flex: none;
  }
  .mark {
    width: 8px;
    height: 8px;
    border-radius: 99px;
    flex: none;
  }
  .mark.ok {
    background: var(--ok);
  }
  .mark.warn {
    background: var(--warn);
  }
  hr {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 4px 2px;
  }
</style>
