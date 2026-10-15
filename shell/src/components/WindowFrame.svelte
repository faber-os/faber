<script lang="ts">
  import type { Snippet } from 'svelte';
  import { moveRect, resizeRect, type Edge, type WindowState } from '../lib/desktop/windows';
  import { t } from '../lib/i18n';
  import { useDesktop } from './desktopController.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { menuTrigger, useMenu, type MenuRequest } from './menuController.svelte';

  let { win, icon, children }: { win: WindowState; icon: IconName; children: Snippet } = $props();

  const desktop = useDesktop();
  const title = $derived(desktop.titleOf(win));
  const menu = useMenu();
  const KEY_STEP = 16;
  const EDGES: Edge[] = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'];

  let section: HTMLDivElement | undefined = $state();
  let interacting = $state(false);

  const focused = $derived(desktop.state.focusedId === win.id);
  const tabs = $derived(desktop.tabs[win.id]);
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

  function titleMenu(target: HTMLElement): MenuRequest {
    const tabId = target.closest<HTMLElement>('[data-tab]')?.dataset.tab;
    const tab = tabId ? tabs?.state.tabs.find((x) => x.id === tabId) : undefined;
    if (tab && tabs) {
      return {
        label: tab.title,
        header: { title: tab.title, icon },
        entries: [
          tabs.add
            ? item('new-tab', tabs.addLabel ?? t('window.tab.new'), { icon: 'plus' })
            : false,
          item('close-tab', t('window.tab.close'), { icon: 'close', shortcut: 'Ctrl+W' }),
        ],
        onSelect: (id: string) => {
          if (id === 'new-tab') tabs.add?.();
          else if (id === 'close-tab') tabs.close(tab.id);
        },
      };
    }
    return {
      label: title,
      entries: [
        item('minimise', t('window.menu.minimise'), { icon: 'minimise' }),
        item('maximise', win.maximised ? t('window.menu.restore') : t('window.menu.maximise'), {
          icon: win.maximised ? 'restore' : 'maximise',
        }),
        separator(),
        item('close', t('window.menu.close'), { icon: 'close' }),
      ],
      onSelect: (id: string) => {
        if (id === 'minimise') desktop.minimise(win.id);
        else if (id === 'maximise') desktop.toggleMaximise(win.id);
        else if (id === 'close') desktop.requestClose(win.id);
      },
    };
  }

  function tabKey(e: KeyboardEvent, index: number) {
    if (!tabs) return;
    const list = tabs.state.tabs;
    let next = -1;
    if (e.key === 'ArrowRight') next = (index + 1) % list.length;
    else if (e.key === 'ArrowLeft') next = (index - 1 + list.length) % list.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = list.length - 1;
    else if (e.key === 'Delete') {
      e.preventDefault();
      e.stopPropagation();
      tabs.close(list[index]!.id);
      return;
    }
    if (next === -1) return;
    e.preventDefault();
    e.stopPropagation();
    tabs.activate(list[next]!.id);
    const strip = (e.currentTarget as HTMLElement).closest('[role="tablist"]');
    strip?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
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
    aria-label={t('window.titlebar.label', { title: title })}
    onpointerdown={startMove}
    ondblclick={(e) => {
      if (!(e.target as Element).closest('button')) desktop.toggleMaximise(win.id);
    }}
    onkeydown={titleKey}
    use:menuTrigger={{ menu, build: titleMenu }}
  >
    {#if tabs}
      <h2 id={titleId} class="visually-hidden">{title}</h2>
      <div class="tabs" role="tablist" aria-label={t('window.tabs.label', { title: title })}>
        {#each tabs.state.tabs as tab, i (tab.id)}
          {@const on = tab.id === tabs.state.activeId}
          <div class="tab" class:on data-tab={tab.id}>
            <button
              class="tab-pick"
              role="tab"
              aria-selected={on}
              tabindex={on ? 0 : -1}
              title={tab.title}
              onclick={() => tabs.activate(tab.id)}
              onauxclick={(e) => {
                if (e.button === 1) tabs.close(tab.id);
              }}
              onkeydown={(e) => tabKey(e, i)}
            >
              {#if tab.dirty}<span class="dirty" title={t('window.tab.unsaved')}></span>{/if}
              <span class="tab-title">{tab.title}</span>
            </button>
            <button
              class="tab-close"
              aria-label={t('window.tab.closeNamed', { title: tab.title })}
              tabindex="-1"
              onclick={() => tabs.close(tab.id)}><Icon name="close" size={14} /></button
            >
          </div>
        {/each}
        {#if tabs.add}
          <button
            class="tab-add"
            aria-label={tabs.addLabel ?? t('window.tab.new')}
            title={tabs.addLabel ?? t('window.tab.new')}
            onclick={() => tabs.add?.()}><Icon name="plus" size={16} /></button
          >
        {/if}
      </div>
    {:else}
      <div class="tabs">
        <div class="tab on solo">
          <span class="title-icon"><Icon name={icon} size={15} /></span>
          <h2 id={titleId} class="title">{title}</h2>
        </div>
      </div>
    {/if}
    <div class="controls">
      <button
        class="ctl"
        aria-label={t('window.control.minimise', { title: title })}
        onclick={() => desktop.minimise(win.id)}
      >
        <Icon name="minimise" size={16} />
      </button>
      <button
        class="ctl"
        aria-label={win.maximised
          ? t('window.control.restore', { title: title })
          : t('window.control.maximise', { title: title })}
        onclick={() => desktop.toggleMaximise(win.id)}
      >
        <Icon name={win.maximised ? 'restore' : 'maximise'} size={16} />
      </button>
      <button
        class="ctl close"
        aria-label={t('window.control.close', { title: title })}
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
    align-items: stretch;
    gap: var(--space-2);
    flex: none;
    height: var(--titlebar-h);
    padding-left: var(--space-2);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    background: var(--chrome-tabs);
    cursor: default;
    touch-action: none;
  }
  .maximised .titlebar {
    border-radius: 0;
  }
  .titlebar:focus-visible {
    outline-offset: -2px;
  }
  .tabs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    display: flex;
    align-items: center;
    min-width: 0;
    max-width: 220px;
    height: calc(var(--titlebar-h) - 6px);
    border-radius: var(--radius-md) var(--radius-md) 0 0;
    color: var(--text-muted);
  }
  .tab:not(.on):hover {
    background: var(--hover);
  }
  .tab.on {
    background: var(--surface);
    color: var(--text);
  }
  .tab.solo {
    gap: var(--space-2);
    padding: 0 var(--space-4) 0 var(--space-3);
    max-width: 100%;
  }
  .tab-pick {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 100%;
    padding: 0 4px 0 var(--space-3);
    border: 0;
    background: none;
    color: inherit;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .tab.on .tab-pick {
    font-weight: 680;
  }
  .tab-pick:focus-visible {
    outline-offset: -3px;
  }
  .tab-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dirty {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }
  .tab-close,
  .tab-add {
    display: grid;
    place-items: center;
    flex: none;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
  }
  .tab-close {
    width: 26px;
    height: 26px;
    margin-right: 4px;
  }
  .tab-add {
    align-self: center;
    width: 30px;
    height: 30px;
  }
  .tab-close:hover,
  .tab-add:hover {
    background: var(--hover);
    color: var(--text);
  }
  .title-icon {
    display: inline-flex;
    color: var(--text-faint);
  }
  .focused .title-icon {
    color: var(--accent-ink);
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 650;
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
    width: 44px;
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
