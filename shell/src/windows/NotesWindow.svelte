<script lang="ts" module>
  let untitled = 0;
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { baseName } from '../lib/desktop/filePaths';
  import {
    activateTab,
    activeTab,
    addTab,
    closeTab,
    dirtyTabs,
    emptyTabs,
    findTab,
    stepTab,
    updateTab,
    type TabsState,
  } from '../lib/desktop/tabs';
  import { untitledName } from '../lib/desktop/textDoc';
  import { t } from '../lib/i18n';
  import { useDesktop } from '../components/desktopController.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import NotesDoc from './NotesDoc.svelte';

  /**
   * A Notes window: one document per tab, the tabs in the title bar. Each tab asks before it
   * closes with unsaved changes; closing the window asks for each such tab in turn.
   */
  let { win }: WindowProps = $props();

  const desktop = useDesktop();

  interface DocTab {
    id: string;
    path: string | null;
    untitled: number;
  }

  let tabs = $state.raw<TabsState>(emptyTabs());
  let docs = $state.raw<DocTab[]>([]);
  const askers = new Map<string, () => boolean>();
  /** The window's close button was used: keep asking about dirty tabs, then close. */
  let closingWindow = false;

  function addDoc(path: string | null) {
    const existing = path ? findTab(tabs, path) : undefined;
    if (existing) {
      tabs = activateTab(tabs, existing.id);
      return;
    }
    const n = path ? 0 : ++untitled;
    const r = addTab(tabs, {
      title: path ? baseName(path) : untitledName(n),
      key: path ?? `new:${n}`,
    });
    tabs = r.state;
    docs = [...docs, { id: r.id, path, untitled: n }];
  }

  // svelte-ignore state_referenced_locally (the file this window was opened for)
  untrack(() => addDoc(win.params.path ?? null));

  // Another file opened in this window (fileOps.openInNotes): a new tab for it.
  $effect(() => {
    const open = win.params.open;
    if (!open) return;
    untrack(() => {
      addDoc(open);
      desktop.update(win.id, { params: {} });
    });
  });

  function requestCloseTab(id: string) {
    const ask = askers.get(id);
    if (ask && !ask()) {
      tabs = activateTab(tabs, id);
      return;
    }
    removeTab(id);
  }

  function removeTab(id: string) {
    tabs = closeTab(tabs, id);
    docs = docs.filter((d) => d.id !== id);
    askers.delete(id);
    if (tabs.tabs.length === 0) desktop.close(win.id);
    else if (closingWindow) askNextOrClose();
  }

  function askNextOrClose() {
    const next = dirtyTabs(tabs)[0];
    if (!next) {
      closingWindow = false;
      desktop.close(win.id);
      return;
    }
    tabs = activateTab(tabs, next.id);
    // Shows that tab's "Save changes?" question; its answer comes back as a close or a cancel.
    askers.get(next.id)?.();
  }

  $effect(() =>
    desktop.setCloseGuard(win.id, () => {
      if (dirtyTabs(tabs).length === 0) return true;
      closingWindow = true;
      askNextOrClose();
      return false;
    }),
  );

  // The title bar draws the tabs.
  $effect(() => {
    desktop.setTabs(win.id, {
      state: tabs,
      activate: (id) => (tabs = activateTab(tabs, id)),
      close: requestCloseTab,
      add: () => addDoc(null),
      addLabel: t('notes.tabs.add'),
    });
  });
  $effect(() => () => desktop.setTabs(win.id, null));

  // The taskbar names the window after its front tab; a leading * marks unsaved changes.
  $effect(() => {
    const tab = activeTab(tabs);
    if (tab) {
      const name = `${tab.dirty ? '*' : ''}${tab.title}`;
      desktop.update(win.id, { title: t('notes.window.title', { name }) });
    }
  });

  function keys(e: KeyboardEvent) {
    // Ctrl+Tab is the browser's own; Ctrl+PageDown/PageUp steps through the tabs instead.
    if ((e.ctrlKey || e.metaKey) && (e.key === 'PageDown' || e.key === 'PageUp')) {
      e.preventDefault();
      tabs = stepTab(tabs, e.key === 'PageDown' ? 1 : -1);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Ctrl+PageUp/PageDown change tabs) -->
<div class="notes-window" onkeydown={keys}>
  {#each docs as d (d.id)}
    <NotesDoc
      id="{win.id}-{d.id}"
      path={d.path}
      untitled={d.untitled}
      active={d.id === tabs.activeId}
      onstate={(s) => (tabs = updateTab(tabs, d.id, s))}
      onopen={(path) => addDoc(path)}
      onnew={() => addDoc(null)}
      onclosed={() => removeTab(d.id)}
      oncancelclose={() => (closingWindow = false)}
      registerClose={(ask) => {
        askers.set(d.id, ask);
        return () => {
          if (askers.get(d.id) === ask) askers.delete(d.id);
        };
      }}
    />
  {/each}
</div>

<style>
  .notes-window {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
</style>
