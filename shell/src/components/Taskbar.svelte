<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { t } from '../lib/i18n';
  import { item, separator, type MenuEntry } from '../lib/desktop/contextMenu';
  import { taskClick, taskbarItems, type TaskItem } from '../lib/desktop/taskbar';
  import AppIcon from './AppIcon.svelte';
  import Clock from './Clock.svelte';
  import EstopButton from './EstopButton.svelte';
  import EstopRelease from './EstopRelease.svelte';
  import ConnectionIndicator from './ConnectionIndicator.svelte';
  import { useDesktop } from './desktopController.svelte';
  import Logo from './Logo.svelte';
  import { menuTrigger, useMenu, type MenuRequest } from './menuController.svelte';
  import { pinItem, pinnedKeys, programsOf, runPinItem, setTaskbarSize } from './programs';
  import StartMenu from './StartMenu.svelte';
  import type { WindowDef } from './windowDefs';

  /**
   * Full width: the start button (the Faber nut) on the left, programs in the middle, connection
   * and clock on the right. Small shows icons; Large (Settings → Desktop) shows bigger buttons with
   * labels, for the 7" touch screen.
   */
  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();

  let menuOpen = $state(false);
  let releasing = $state(false);
  let startButton: HTMLButtonElement | undefined = $state();
  let root: HTMLDivElement | undefined = $state();

  const large = $derived(core.settings?.taskbar.size === 'large');
  const programs = $derived(programsOf(defs));
  const items = $derived(
    taskbarItems(pinnedKeys(core), programs, desktop.state.windows, desktop.state.focusedId),
  );

  function closeMenu(returnFocus: boolean) {
    menuOpen = false;
    if (returnFocus) startButton?.focus();
  }

  function def(kind: string): WindowDef | undefined {
    return defs.find((d) => d.kind === kind);
  }

  function itemKey(i: TaskItem): string {
    return i.type === 'program' ? i.key : i.id;
  }

  function label(i: TaskItem): string {
    if (i.type === 'program') return def(i.kind)?.title ?? i.key;
    return (() => {
      const w = desktop.state.windows.find((x) => x.id === i.id);
      return w ? desktop.titleOf(w) : '';
    })();
  }

  function isMinimised(i: TaskItem): boolean {
    return i.type === 'window' && !!desktop.state.windows.find((w) => w.id === i.id)?.minimised;
  }

  function click(i: TaskItem) {
    const c = taskClick(i, desktop.state.focusedId);
    if (c.do === 'open') desktop.open(c.kind);
    else desktop.activate(c.id);
  }

  function windowMenu(id: string): MenuRequest | null {
    const w = desktop.state.windows.find((x) => x.id === id);
    if (!w) return null;
    const program = def(w.kind)?.program;
    return {
      label: desktop.titleOf(w),
      header: { title: desktop.titleOf(w), icon: def(w.kind)?.icon ?? 'info' },
      entries: [
        w.minimised
          ? item('show', t('window.menu.show'), { icon: 'maximise' })
          : item('minimise', t('window.menu.minimise'), { icon: 'minimise' }),
        item('maximise', w.maximised ? t('window.menu.restore') : t('window.menu.maximise'), {
          icon: w.maximised ? 'restore' : 'maximise',
        }),
        program ? pinItem(core, program) : false,
        separator(),
        item('close', t('window.menu.close'), { icon: 'close' }),
      ],
      onSelect: (choice: string) => {
        if (program && runPinItem(core, program, choice)) return;
        if (choice === 'show') desktop.focus(w.id);
        else if (choice === 'minimise') desktop.minimise(w.id);
        else if (choice === 'maximise') desktop.toggleMaximise(w.id);
        else if (choice === 'close') desktop.requestClose(w.id);
      },
    };
  }

  function programMenu(i: Extract<TaskItem, { type: 'program' }>): MenuRequest {
    const d = def(i.kind);
    const wins = i.windows.flatMap((id) => {
      const w = desktop.state.windows.find((x) => x.id === id);
      return w ? [w] : [];
    });
    const entries: (MenuEntry | false)[] = [
      ...wins.map((w) => item(`win:${w.id}`, desktop.titleOf(w), { icon: d?.icon ?? 'info' })),
      wins.length > 0 && separator(),
      wins.length === 0
        ? item('new', t('menu.open'), { icon: 'open' })
        : !!d?.multiple && item('new', t('taskbar.menu.newWindow'), { icon: 'newWindow' }),
      pinItem(core, i.key),
      wins.length > 0 && separator(),
      wins.length === 1 && item('close', t('taskbar.menu.closeWindow'), { icon: 'close' }),
      wins.length > 1 && item('close', t('taskbar.menu.closeAllWindows'), { icon: 'close' }),
    ];
    return {
      label: d?.title ?? i.key,
      header: { title: d?.title ?? i.key, detail: d?.description, icon: d?.icon ?? 'info' },
      entries,
      onSelect: (choice: string) => {
        if (runPinItem(core, i.key, choice)) return;
        if (choice.startsWith('win:')) desktop.focus(choice.slice(4));
        else if (choice === 'new')
          desktop.open(i.kind, d?.multiple ? { key: `${i.kind}:${Date.now()}` } : {});
        else if (choice === 'close') for (const w of wins) desktop.requestClose(w.id);
      },
    };
  }

  function barMenu(): MenuRequest {
    return {
      label: t('taskbar.label'),
      entries: [
        item('small', t('taskbar.menu.small'), { checked: !large }),
        item('large', t('taskbar.menu.large'), { checked: large }),
        separator(),
        item('settings', t('taskbar.menu.settings'), { icon: 'settings' }),
      ],
      onSelect: (choice: string) => {
        if (choice === 'small' || choice === 'large') void setTaskbarSize(core, choice);
        else if (choice === 'settings')
          desktop.open('settings', { params: { section: 'desktop' } });
      },
    };
  }

  function build(target: HTMLElement): MenuRequest | null {
    if (target.closest('.start')) return null;
    const key = target.closest<HTMLElement>('[data-task]')?.dataset.task;
    if (!key) return barMenu();
    const i = items.find((x) => itemKey(x) === key);
    if (!i) return null;
    return i.type === 'program' ? programMenu(i) : windowMenu(i.id);
  }

  function onWindowPointer(e: PointerEvent) {
    if (menuOpen && root && !root.contains(e.target as Node)) menuOpen = false;
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<!-- The start menu sits beside the bar, not inside it: a blurred element inside another blurred
     element can only blur its parent, not the windows behind it. -->
<div class="layer" bind:this={root}>
  <nav
    class="taskbar"
    class:large
    aria-label={t('taskbar.label')}
    use:menuTrigger={{ menu, build }}
  >
    <div class="side left">
      <button
        bind:this={startButton}
        class="start"
        class:open={menuOpen}
        aria-haspopup="dialog"
        aria-expanded={menuOpen}
        aria-controls="start-menu"
        aria-label={t('startMenu.label')}
        onclick={() => (menuOpen = !menuOpen)}
      >
        <span class="nut"><Logo size={large ? 28 : 24} /></span>
        {#if large}<span class="start-label" aria-hidden="true">Faber</span>{/if}
      </button>
    </div>

    <ul class="tasks" aria-label={t('taskbar.tasks')}>
      {#each items as i (itemKey(i))}
        {@const d = def(i.kind)}
        {@const open = i.type === 'window' || i.windows.length > 0}
        {@const name = label(i)}
        <li>
          <button
            class="task"
            class:open
            class:active={i.active}
            class:minimised={isMinimised(i)}
            aria-pressed={i.active}
            aria-label={name}
            title={i.type === 'program' && i.windows.length > 1
              ? t('taskbar.task.windows', { name, count: i.windows.length })
              : name}
            data-task={itemKey(i)}
            onclick={() => click(i)}
          >
            {#if large}
              <AppIcon icon={d?.icon ?? 'info'} colour={d?.colour ?? 'about'} size={30} />
              <span class="task-title" aria-hidden="true">{name}</span>
            {:else}
              <AppIcon icon={d?.icon ?? 'info'} colour={d?.colour ?? 'about'} size={22} flat />
            {/if}
            <span class="mark" aria-hidden="true"></span>
          </button>
        </li>
      {/each}
    </ul>

    <div class="side right">
      <EstopButton {large} onrelease={() => (releasing = true)} />
      <ConnectionIndicator status={core.connection} />
      <Clock withDate={large} />
    </div>
  </nav>
  {#if menuOpen}
    <StartMenu {defs} onclose={closeMenu} />
  {/if}
  {#if releasing}
    <EstopRelease onclose={() => (releasing = false)} />
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
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    background: var(--taskbar);
    border-top: 1px solid var(--line);
    z-index: 99999;
  }
  .side {
    display: flex;
    align-items: center;
    min-width: 0;
  }
  .right {
    justify-content: flex-end;
    gap: var(--space-1);
  }
  .start {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-width: 48px;
    height: var(--touch);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    transition: background var(--dur-fast);
  }
  .large .start {
    height: 52px;
    padding: 0 var(--space-3) 0 var(--space-2);
  }
  .start-label {
    font-size: var(--text-lg);
    font-weight: 750;
    letter-spacing: -0.01em;
  }
  .nut {
    display: inline-flex;
    color: var(--accent);
    transition: transform var(--dur-med) var(--ease-out);
  }
  /* A quarter-turn of a nut is 60°: the start button turns one flat when its menu opens. */
  .start.open .nut {
    transform: rotate(60deg);
  }
  .start:hover {
    background: var(--hover);
  }
  .start.open {
    background: var(--press);
  }
  .tasks {
    display: flex;
    justify-content: center;
    gap: 2px;
    min-width: 0;
    max-width: calc(100vw - 280px);
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .task {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    width: 46px;
    height: 42px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    transition: background var(--dur-fast);
  }
  .task:hover {
    background: var(--hover);
  }
  .task.active {
    background: var(--surface);
    box-shadow:
      0 0 0 1px var(--line),
      0 1px 2px oklch(0.2 0.01 250 / 0.1);
  }
  .task.minimised {
    opacity: 0.7;
  }
  /* Open: a short dot under the icon. In front: a longer accent bar. */
  .mark {
    position: absolute;
    left: 50%;
    bottom: 2px;
    width: 0;
    height: 3px;
    margin-left: 0;
    border-radius: 2px;
    background: var(--text-faint);
    transition:
      width var(--dur-med) var(--ease-out),
      margin var(--dur-med) var(--ease-out),
      background var(--dur-fast);
  }
  .task.open .mark {
    width: 6px;
    margin-left: -3px;
  }
  .task.active .mark {
    width: 16px;
    margin-left: -8px;
    background: var(--accent);
  }

  .large .task {
    width: auto;
    min-width: 128px;
    max-width: 200px;
    height: 54px;
    justify-content: flex-start;
    padding: 0 var(--space-3) 0 var(--space-2);
    color: var(--text);
  }
  .task-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .large .task.active .mark {
    width: 24px;
    margin-left: -12px;
  }
  /* The 7" screen and smaller: large buttons get narrower, then drop their labels. */
  @media (max-width: 1100px) {
    .large .task {
      min-width: 0;
      max-width: 150px;
    }
  }
  @media (max-width: 640px) {
    .large .task {
      width: 56px;
      justify-content: center;
      padding: 0;
    }
    .large .task-title {
      display: none;
    }
  }
</style>
