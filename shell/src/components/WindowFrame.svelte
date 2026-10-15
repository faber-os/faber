<script lang="ts">
  import type { Snippet } from 'svelte';
  import { moveRect, resizeRect, type Edge, type WindowState } from '../lib/desktop/windows';
  import { useDesktop } from './desktopController.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { menuTrigger, useMenu } from './menuController.svelte';

  let { win, icon, children }: { win: WindowState; icon: IconName; children: Snippet } = $props();

  const desktop = useDesktop();
  const menu = useMenu();
  const KEY_STEP = 16;
  const EDGES: Edge[] = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'];

  let section: HTMLDivElement | undefined = $state();
  let interacting = $state(false);

  const focused = $derived(desktop.state.focusedId === win.id);
  const box = $derived(
    win.maximised ? { x: 0, y: 0, w: desktop.area.width, h: desktop.area.height } : win.rect,
  );
  const titleId = $derived(`${win.id}-title`);

  // Keyboard users land in a window when it comes to the front (opened, or picked in the taskbar).
  $effect(() => {
    if (focused && !win.minimised && section && !section.contains(document.activeElement)) {
      section.focus({ preventScroll: true });
    }
  });

  /** Follow one pointer until it is released, reporting the distance moved. */
  function track(e: PointerEvent, onMove: (dx: number, dy: number) => void) {
    const el = e.currentTarget as HTMLElement;
    const sx = e.clientX;
    const sy = e.clientY;
    el.setPointerCapture(e.pointerId);
    interacting = true;
    const move = (ev: PointerEvent) => onMove(ev.clientX - sx, ev.clientY - sy);
    const end = () => {
      interacting = false;
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', end);
      el.removeEventListener('pointercancel', end);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', end);
    el.addEventListener('pointercancel', end);
  }

  function startMove(e: PointerEvent) {
    if (e.button !== 0 || win.maximised) return;
    if ((e.target as Element).closest('button')) return;
    const start = win.rect;
    track(e, (dx, dy) => desktop.setRect(win.id, moveRect(start, dx, dy, desktop.area)));
  }

  function startResize(e: PointerEvent, edge: Edge) {
    if (e.button !== 0) return;
    e.stopPropagation();
    const start = win.rect;
    const min = desktop.kind(win).minSize;
    track(e, (dx, dy) =>
      desktop.setRect(win.id, resizeRect(start, edge, dx, dy, min, desktop.area)),
    );
  }

  function titleMenu() {
    return {
      label: win.title,
      entries: [
        item('minimise', 'Minimise'),
        item('maximise', win.maximised ? 'Restore size' : 'Maximise'),
        separator(),
        item('close', 'Close'),
      ],
      onSelect: (id: string) => {
        if (id === 'minimise') desktop.minimise(win.id);
        else if (id === 'maximise') desktop.toggleMaximise(win.id);
        else if (id === 'close') desktop.requestClose(win.id);
      },
    };
  }

  function titleKey(e: KeyboardEvent) {
    const d: Record<string, [number, number]> = {
      ArrowLeft: [-KEY_STEP, 0],
      ArrowRight: [KEY_STEP, 0],
      ArrowUp: [0, -KEY_STEP],
      ArrowDown: [0, KEY_STEP],
    };
    const delta = d[e.key];
    if (delta) {
      if (win.maximised) return;
      e.preventDefault();
      const [dx, dy] = delta;
      const rect = e.shiftKey
        ? resizeRect(win.rect, 'se', dx, dy, desktop.kind(win).minSize, desktop.area)
        : moveRect(win.rect, dx, dy, desktop.area);
      desktop.setRect(win.id, rect);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      desktop.toggleMaximise(win.id);
    }
  }
</script>

<div
  bind:this={section}
  class="window"
  class:focused
  class:maximised={win.maximised}
  class:interacting
  role="dialog"
  aria-modal="false"
  aria-labelledby={titleId}
  tabindex="-1"
  hidden={win.minimised}
  data-window-kind={win.kind}
  style:left="{box.x}px"
  style:top="{box.y}px"
  style:width="{box.w}px"
  style:height="{box.h}px"
  style:z-index={win.z}
  onpointerdowncapture={() => desktop.focus(win.id)}
>
  <header
    class="titlebar"
    role="toolbar"
    tabindex="0"
    aria-label="{win.title} window. Arrow keys move it, Shift and arrow keys resize it, Enter maximises it."
    onpointerdown={startMove}
    ondblclick={(e) => {
      if (!(e.target as Element).closest('button')) desktop.toggleMaximise(win.id);
    }}
    onkeydown={titleKey}
    use:menuTrigger={{ menu, build: titleMenu }}
  >
    <span class="title-icon"><Icon name={icon} size={16} /></span>
    <h2 id={titleId} class="title">{win.title}</h2>
    <div class="controls">
      <button
        class="ctl"
        aria-label="Minimise {win.title}"
        onclick={() => desktop.minimise(win.id)}
      >
        <Icon name="minimise" size={16} />
      </button>
      <button
        class="ctl"
        aria-label="{win.maximised ? 'Restore' : 'Maximise'} {win.title}"
        onclick={() => desktop.toggleMaximise(win.id)}
      >
        <Icon name={win.maximised ? 'restore' : 'maximise'} size={16} />
      </button>
      <button
        class="ctl close"
        aria-label="Close {win.title}"
        onclick={() => desktop.requestClose(win.id)}
      >
        <Icon name="close" size={16} />
      </button>
    </div>
  </header>
  <div class="body">
    {@render children()}
  </div>
  {#if !win.maximised}
    {#each EDGES as edge (edge)}
      <div class="grip {edge}" aria-hidden="true" onpointerdown={(e) => startResize(e, edge)}></div>
    {/each}
  {/if}
</div>

<style>
  .window {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-window);
    animation: open var(--dur-med) var(--ease-out);
    outline: none;
  }
  .window[hidden] {
    display: none;
  }
  .window.focused {
    box-shadow: var(--shadow-window-focus);
  }
  .window.maximised {
    border-radius: 0;
  }
  /* The window itself only receives focus so keyboard users land in it; the focused window is
     already marked by its deeper shadow and teal title icon. */
  .window:focus-visible {
    outline: none;
  }
  .window.interacting {
    user-select: none;
  }
  @keyframes open {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.985);
    }
  }

  .titlebar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: var(--titlebar-h);
    padding-left: var(--space-4);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    cursor: default;
    touch-action: none;
  }
  .maximised .titlebar {
    border-radius: 0;
  }
  .titlebar:focus-visible {
    outline-offset: -2px;
  }
  .title-icon {
    display: inline-flex;
    color: var(--text-faint);
  }
  .focused .title-icon {
    color: var(--accent-ink);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 620;
    letter-spacing: -0.005em;
    color: var(--text-muted);
  }
  .focused .title {
    color: var(--text);
  }
  .controls {
    display: flex;
    align-self: stretch;
  }
  .ctl {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    transition: background var(--dur-fast);
  }
  .ctl:hover {
    background: var(--hover);
    color: var(--text);
  }
  .ctl:active {
    background: var(--press);
  }
  .ctl.close {
    border-top-right-radius: var(--radius-lg);
  }
  .maximised .ctl.close {
    border-radius: 0;
  }
  .ctl.close:hover {
    background: var(--danger);
    color: var(--surface);
  }
  .ctl:focus-visible {
    outline-offset: -3px;
  }

  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    overflow: hidden;
  }
  .maximised .body {
    border-radius: 0;
  }

  .grip {
    position: absolute;
    touch-action: none;
  }
  .n,
  .s {
    left: 10px;
    right: 10px;
    height: 8px;
    cursor: ns-resize;
  }
  .n {
    top: -4px;
  }
  .s {
    bottom: -4px;
  }
  .e,
  .w {
    top: 10px;
    bottom: 10px;
    width: 8px;
    cursor: ew-resize;
  }
  .e {
    right: -4px;
  }
  .w {
    left: -4px;
  }
  .ne,
  .nw,
  .se,
  .sw {
    width: 16px;
    height: 16px;
  }
  .ne {
    top: -5px;
    right: -5px;
    cursor: nesw-resize;
  }
  .sw {
    bottom: -5px;
    left: -5px;
    cursor: nesw-resize;
  }
  .nw {
    top: -5px;
    left: -5px;
    cursor: nwse-resize;
  }
  .se {
    bottom: -5px;
    right: -5px;
    cursor: nwse-resize;
  }
  @media (pointer: coarse) {
    .n,
    .s {
      height: 16px;
    }
    .e,
    .w {
      width: 16px;
    }
    .ne,
    .nw,
    .se,
    .sw {
      width: 28px;
      height: 28px;
    }
  }
</style>
