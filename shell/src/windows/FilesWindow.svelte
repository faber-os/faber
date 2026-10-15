<script lang="ts">
  import { untrack } from 'svelte';
  import {
    activateTab,
    activeTab,
    addTab,
    closeTab,
    emptyTabs,
    stepTab,
    updateTab,
    type TabsState,
  } from '../lib/desktop/tabs';
  import { baseName } from '../lib/desktop/filePaths';
  import { t } from '../lib/i18n';
  import { useDesktop } from '../components/desktopController.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import FilesPane from './FilesPane.svelte';

  /** A Files window: one folder per tab, the tabs in the title bar. */
  let { win }: WindowProps = $props();

  const desktop = useDesktop();
  const HOME_START = 'home:/Desktop';

  let tabs = $state.raw<TabsState>(emptyTabs());
  let panes = $state.raw<{ id: string; start: string }[]>([]);

  function addPane(start: string) {
    const r = addTab(tabs, { title: baseName(start) || t('files.home'), key: start });
    tabs = r.state;
    panes = [...panes, { id: r.id, start }];
  }

  // svelte-ignore state_referenced_locally (the folder this window was opened for)
  untrack(() => addPane(win.params.path ?? HOME_START));

  function close(id: string) {
    tabs = closeTab(tabs, id);
    panes = panes.filter((p) => p.id !== id);
    if (tabs.tabs.length === 0) desktop.close(win.id);
  }

  $effect(() => {
    desktop.setTabs(win.id, {
      state: tabs,
      activate: (id) => (tabs = activateTab(tabs, id)),
      close,
      add: () => addPane(HOME_START),
      addLabel: t('files.newTab'),
    });
  });
  $effect(() => () => desktop.setTabs(win.id, null));

  // The taskbar tells two Files windows apart by their front tab's folder.
  $effect(() => {
    const tab = activeTab(tabs);
    if (tab) desktop.update(win.id, { title: t('files.windowTitle.folder', { name: tab.title }) });
  });

  function keys(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && (e.key === 'PageDown' || e.key === 'PageUp')) {
      e.preventDefault();
      tabs = stepTab(tabs, e.key === 'PageDown' ? 1 : -1);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Ctrl+PageUp/PageDown change tabs) -->
<div class="files-window" onkeydown={keys}>
  {#each panes as p (p.id)}
    <FilesPane
      id="{win.id}-{p.id}"
      start={p.start}
      active={p.id === tabs.activeId}
      onstate={(s) => (tabs = updateTab(tabs, p.id, s))}
      onopentab={addPane}
    />
  {/each}
</div>

<style>
  .files-window {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
</style>
