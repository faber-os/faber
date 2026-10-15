<script lang="ts">
  import { useCore } from '../lib/state/context';
  import Clock from './Clock.svelte';
  import ConnectionIndicator from './ConnectionIndicator.svelte';
  import { useDesktop } from './desktopController.svelte';
  import Icon from './Icon.svelte';
  import StartMenu from './StartMenu.svelte';
  import { useTheme } from './theme.svelte';
  import type { WindowDef } from './windowDefs';
  import Wordmark from './Wordmark.svelte';

  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const theme = useTheme();

  let menuOpen = $state(false);
  let startButton: HTMLButtonElement | undefined = $state();
  let root: HTMLDivElement | undefined = $state();

  const THEME_LABEL = {
    system: 'Theme: follows the system',
    light: 'Theme: light',
    dark: 'Theme: dark',
  } as const;

  function closeMenu(returnFocus: boolean) {
    menuOpen = false;
    if (returnFocus) startButton?.focus();
  }

  function iconFor(kind: string) {
    return defs.find((d) => d.kind === kind)?.icon ?? 'info';
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
      <Wordmark size={19} />
    </button>
    <ul class="tasks" aria-label="Open windows">
      {#each desktop.state.windows as w (w.id)}
        {@const active = desktop.state.focusedId === w.id && !w.minimised}
        <li>
          <button
            class="task"
            class:active
            class:minimised={w.minimised}
            aria-pressed={active}
            title={w.title}
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
      <button
        class="tray-btn"
        aria-label={THEME_LABEL[theme.preference]}
        title={THEME_LABEL[theme.preference]}
        onclick={() => theme.cycle()}
      >
        <Icon
          name={theme.preference === 'system'
            ? 'auto'
            : theme.preference === 'light'
              ? 'sun'
              : 'moon'}
        />
      </button>
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
    backdrop-filter: var(--chrome-blur);
    -webkit-backdrop-filter: var(--chrome-blur);
    border-top: 1px solid var(--line);
    box-shadow: var(--highlight);
    z-index: 99999;
  }
  .start {
    display: inline-flex;
    align-items: center;
    height: var(--touch);
    padding: 0 var(--space-4) 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    transition: background var(--dur-fast);
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
      0 1px 2px oklch(0.2 0.02 255 / 0.14),
      var(--highlight);
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
  .tray-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--touch);
    height: var(--touch);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
  }
  .tray-btn:hover {
    background: var(--hover);
    color: var(--text);
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
