<script lang="ts">
  import { useCore } from '../lib/state/context';
  import Clock from './Clock.svelte';
  import ConnectionIndicator from './ConnectionIndicator.svelte';
  import { useDesktop } from './desktopController.svelte';
  import Icon from './Icon.svelte';
  import { item, separator } from '../lib/desktop/contextMenu';
  import Logo from './Logo.svelte';
  import { menuTrigger, useMenu } from './menuController.svelte';
  import StartMenu from './StartMenu.svelte';
  import type { WindowDef } from './windowDefs';

  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();

  let menuOpen = $state(false);
  let startButton: HTMLButtonElement | undefined = $state();
  let root: HTMLDivElement | undefined = $state();

  function closeMenu(returnFocus: boolean) {
    menuOpen = false;
    if (returnFocus) startButton?.focus();
  }

  function iconFor(kind: string) {
    return defs.find((d) => d.kind === kind)?.icon ?? 'info';
  }

  function taskMenu(target: HTMLElement) {
    const id = target.closest<HTMLElement>('[data-window]')?.dataset.window;
    const w = desktop.state.windows.find((x) => x.id === id);
    if (!w) return null;
    return {
      label: w.title,
      entries: [
        w.minimised ? item('show', 'Show') : item('minimise', 'Minimise'),
        item('maximise', w.maximised ? 'Restore size' : 'Maximise'),
        separator(),
        item('close', 'Close'),
      ],
      onSelect: (choice: string) => {
        if (choice === 'show') desktop.focus(w.id);
        else if (choice === 'minimise') desktop.minimise(w.id);
        else if (choice === 'maximise') desktop.toggleMaximise(w.id);
        else if (choice === 'close') desktop.requestClose(w.id);
      },
    };
  }

  function onWindowPointer(e: PointerEvent) {
    if (menuOpen && root && !root.contains(e.target as Node)) menuOpen = false;
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<!-- The start menu sits beside the bar, not inside it: a blurred element inside another blurred
     element can only blur its parent, not the windows behind it. -->
<div class="layer" bind:this={root}>
  <nav class="taskbar" aria-label="Taskbar">
    <button
      bind:this={startButton}
      class="start"
      class:open={menuOpen}
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      aria-controls="start-menu"
      aria-label="Start"
      onclick={() => (menuOpen = !menuOpen)}
    >
      <span class="nut"><Logo size={24} /></span>
    </button>
    <ul class="tasks" aria-label="Open windows" use:menuTrigger={{ menu, build: taskMenu }}>
      {#each desktop.state.windows as w (w.id)}
        {@const active = desktop.state.focusedId === w.id && !w.minimised}
        <li>
          <button
            class="task"
            class:active
            class:minimised={w.minimised}
            aria-pressed={active}
            title={w.title}
            data-window={w.id}
            onclick={() => desktop.activate(w.id)}
          >
            <Icon name={iconFor(w.kind)} size={17} />
            <span class="task-title">{w.title}</span>
          </button>
        </li>
      {/each}
    </ul>

    <div class="tray">
      <ConnectionIndicator status={core.connection} />
      <Clock />
    </div>
  </nav>
  {#if menuOpen}
    <StartMenu {defs} onclose={closeMenu} />
  {/if}
</div>

<style>
  .layer {
    display: contents;
  }
  .taskbar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: var(--taskbar-h);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    background: var(--taskbar);
    border-top: 1px solid var(--line);
    z-index: 99999;
  }
  .start {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: var(--touch);
    padding: 0;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    transition: background var(--dur-fast);
  }
  .nut {
    color: var(--accent);
    transition: transform var(--dur-med) var(--ease-out);
  }
  /* A quarter-turn of a nut is 60°: the start button turns one flat when its menu opens. */
  .start.open .nut {
    transform: rotate(60deg);
  }
  .start:hover,
  .start.open {
    background: var(--hover);
  }
  .start.open {
    background: var(--press);
  }
  .tasks {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: var(--space-1);
    margin: 0;
    padding: 0 0 0 var(--space-2);
    list-style: none;
    overflow-x: auto;
    scrollbar-width: none;
    border-left: 1px solid var(--line);
  }
  .tasks li {
    min-width: 0;
    flex: 0 1 200px;
  }
  .task {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-width: 48px;
    height: var(--touch);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    font-size: var(--text-sm);
    font-weight: 560;
    transition: background var(--dur-fast);
  }
  .task:hover {
    background: var(--hover);
    color: var(--text);
  }
  .task.active {
    background: var(--surface);
    color: var(--text);
    box-shadow:
      0 0 0 1px var(--line),
      0 1px 2px oklch(0.2 0.01 250 / 0.1);
  }
  .task::after {
    content: '';
    position: absolute;
    left: 50%;
    bottom: 3px;
    width: 14px;
    height: 3px;
    margin-left: -7px;
    border-radius: 2px;
    background: var(--text-faint);
    transition:
      width var(--dur-med) var(--ease-out),
      background var(--dur-fast);
  }
  .task.active::after {
    width: 22px;
    margin-left: -11px;
    background: var(--accent);
  }
  .task.minimised {
    opacity: 0.75;
  }
  .task-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tray {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  @media (max-width: 640px) {
    .task-title {
      display: none;
    }
    .tasks li {
      flex: none;
    }
  }
</style>
