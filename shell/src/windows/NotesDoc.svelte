<script lang="ts">
  import { tick } from 'svelte';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import { DESKTOP, parentPath } from '../lib/desktop/filePaths';
  import {
    decodeTextFile,
    edit,
    encodeText,
    findMatches,
    isDirty,
    lineColumn,
    markSaved,
    matchAt,
    newDoc,
    openedDoc,
    stepMatch,
    type TextDoc,
  } from '../lib/desktop/textDoc';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import FilePicker from '../components/FilePicker.svelte';
  import Icon from '../components/Icon.svelte';

  /**
   * One document in a Notes window: a tab's editor, toolbar, find bar and save dialogs. The
   * window (NotesWindow) owns the tabs; this reports its name and unsaved state to it.
   */
  let {
    id,
    path: initialPath,
    untitled,
    active,
    onstate,
    onopen,
    onnew,
    onclosed,
    oncancelclose,
    registerClose,
  }: {
    /** Unique per tab, for element ids. */
    id: string;
    /** The file this tab was opened for, or null for a new note. */
    path: string | null;
    /** The number of a new note ("Untitled 2"). */
    untitled: number;
    /** The tab in front. The others stay mounted (keeping their text) but hidden. */
    active: boolean;
    onstate: (s: { title: string; dirty: boolean; key: string }) => void;
    /** Open another file (Open…), in a tab of this window. */
    onopen: (path: string) => void;
    /** A new empty note, in a tab of this window. */
    onnew: () => void;
    /** This tab is done: closed after asking, or given up after an error. */
    onclosed: () => void;
    /** The user chose Cancel when asked about unsaved changes. */
    oncancelclose: () => void;
    /** The window asks before closing the tab: false means "not yet, I'm asking the user". */
    registerClose: (ask: () => boolean) => () => void;
  } = $props();

  const core = useCore();

  // svelte-ignore state_referenced_locally (a tab keeps the file it was opened for)
  const openedPath = initialPath;
  // svelte-ignore state_referenced_locally (the file this tab was opened for, and its number)
  let doc = $state.raw<TextDoc>(openedPath ? openedDoc(openedPath, '', '\n') : newDoc(untitled));
  // svelte-ignore state_referenced_locally
  const newKey = `new:${untitled}`;
  let loading = $state(openedPath !== null);
  let loadError = $state<ErrorBody | null>(null);
  let saveError = $state<ErrorBody | null>(null);
  let saving = $state(false);
  let picker = $state<'open' | 'save' | null>(null);
  let asking = $state<'close' | null>(null);
  let wrap = $state(true);
  let mono = $state(true);
  let cursor = $state(0);
  let findOpen = $state(false);
  let query = $state('');
  let matchCase = $state(false);
  let current = $state(-1);
  let ta: HTMLTextAreaElement | undefined = $state();
  let findInput: HTMLInputElement | undefined = $state();

  const dirty = $derived(isDirty(doc));
  const pos = $derived(lineColumn(doc.text, cursor));
  const matches = $derived(findOpen ? findMatches(doc.text, query, matchCase) : []);

  async function load(path: string) {
    loading = true;
    loadError = null;
    const r = await core.readFile(path);
    loading = false;
    if (!r.ok) {
      loadError = r.error;
      return;
    }
    const d = decodeTextFile(r.value);
    if (!d.ok) {
      loadError = { code: d.reason, message: d.message, hint: d.hint };
      return;
    }
    doc = openedDoc(path, d.text, d.eol);
  }

  if (openedPath) void load(openedPath);

  // The tab shows which file, and whether it has unsaved changes.
  $effect(() => {
    onstate({ title: doc.name, dirty, key: doc.path ?? newKey });
  });

  // Closing with unsaved changes asks first.
  $effect(() =>
    registerClose(() => {
      if (!isDirty(doc)) return true;
      asking = 'close';
      return false;
    }),
  );

  async function save(): Promise<boolean> {
    if (!doc.path) {
      picker = 'save';
      return false;
    }
    return writeTo(doc.path);
  }

  async function writeTo(path: string): Promise<boolean> {
    saving = true;
    saveError = null;
    const r = await core.writeFile(path, encodeText(doc.text, doc.eol));
    saving = false;
    if (!r.ok) {
      // e.g. config-invalid for System/parts.toml: the text stays here, unsaved, to be fixed.
      saveError = r.error;
      return false;
    }
    doc = markSaved(doc, path);
    return true;
  }

  async function savedAs(path: string) {
    picker = null;
    const ok = await writeTo(path);
    if (ok && asking === 'close') closeNow();
  }

  function cancelClose() {
    asking = null;
    oncancelclose();
  }

  function closeNow() {
    asking = null;
    onclosed();
  }

  async function saveThenClose() {
    asking = null;
    if (await save()) closeNow();
    else if (picker === 'save') asking = 'close'; // Save as is open; close once it's done.
  }

  function onInput(e: Event) {
    const el = e.currentTarget as HTMLTextAreaElement;
    doc = edit(doc, el.value);
    cursor = el.selectionStart;
    if (saveError?.code !== 'config-invalid') saveError = null;
  }

  function trackCursor() {
    if (ta) cursor = ta.selectionStart;
  }

  async function openFind() {
    findOpen = true;
    if (ta && ta.selectionEnd > ta.selectionStart) {
      query = doc.text.slice(ta.selectionStart, ta.selectionEnd);
    }
    await tick();
    findInput?.focus();
    findInput?.select();
  }

  function closeFind() {
    findOpen = false;
    current = -1;
    ta?.focus();
  }

  function step(dir: 'next' | 'prev') {
    if (!ta || matches.length === 0) {
      current = -1;
      return;
    }
    const on = matchAt(matches, ta.selectionStart, ta.selectionEnd);
    const from =
      dir === 'next' ? (on >= 0 ? matches[on]!.end : ta.selectionEnd) : ta.selectionStart;
    current = stepMatch(matches, from, dir);
    const m = matches[current]!;
    // Focusing the text box makes the browser scroll the match into view; then go back to Find.
    ta.focus();
    ta.setSelectionRange(m.start, m.end);
    cursor = m.start;
    findInput?.focus();
  }

  function keys(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (e.shiftKey) picker = 'save';
      else void save();
    } else if (mod && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      void openFind();
    } else if (mod && e.key.toLowerCase() === 'o') {
      e.preventDefault();
      picker = 'open';
    } else if (e.key === 'F3') {
      e.preventDefault();
      if (!findOpen) void openFind();
      else step(e.shiftKey ? 'prev' : 'next');
    }
  }

  const saveStart = $derived(doc.path ? (parentPath(doc.path) ?? DESKTOP) : 'home:/Documents');
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (window-wide shortcuts: Ctrl+S, Ctrl+F, F3) -->
<div class="notes" hidden={!active} onkeydown={keys}>
  <div class="toolbar" role="toolbar" aria-label="Notes">
    <button class="btn quiet" title="New note in a new tab" onclick={onnew}>
      <Icon name="plus" size={17} /> New
    </button>
    <button class="btn quiet" title="Open (Ctrl+O)" onclick={() => (picker = 'open')}>
      <Icon name="open" size={17} /> Open…
    </button>
    <button
      class="btn quiet"
      title="Save (Ctrl+S)"
      disabled={saving || loading || !!loadError}
      onclick={save}
    >
      <Icon name="save" size={17} /> Save
    </button>
    <button
      class="btn quiet"
      title="Save as (Ctrl+Shift+S)"
      disabled={loading || !!loadError}
      onclick={() => (picker = 'save')}>Save as…</button
    >
    <span class="grow"></span>
    <button
      class="btn quiet icon"
      aria-label="Find"
      title="Find (Ctrl+F)"
      aria-pressed={findOpen}
      onclick={() => (findOpen ? closeFind() : openFind())}><Icon name="search" /></button
    >
    <button
      class="btn quiet icon"
      aria-label="Word wrap"
      title="Word wrap"
      aria-pressed={wrap}
      onclick={() => (wrap = !wrap)}><Icon name="wrap" /></button
    >
    <button
      class="btn quiet mono-toggle"
      aria-label="Monospace"
      title="Monospace font"
      aria-pressed={mono}
      onclick={() => (mono = !mono)}><span class="mono">Aa</span></button
    >
  </div>

  {#if findOpen}
    <div class="find" role="search">
      <label class="visually-hidden" for="{id}-find">Find</label>
      <input
        id="{id}-find"
        bind:this={findInput}
        class="input"
        placeholder="Find"
        bind:value={query}
        oninput={() => (current = -1)}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            step(e.shiftKey ? 'prev' : 'next');
          } else if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            closeFind();
          }
        }}
        spellcheck="false"
        autocomplete="off"
      />
      <span class="count num" role="status">
        {#if query === ''}{:else if matches.length === 0}No matches{:else if current >= 0}{current +
            1} of {matches.length}{:else}{matches.length} found{/if}
      </span>
      <label class="case"
        ><input
          type="checkbox"
          class="check"
          bind:checked={matchCase}
          onchange={() => (current = -1)}
        /> Match case</label
      >
      <button
        class="btn quiet icon"
        aria-label="Previous match"
        title="Previous (Shift+Enter)"
        disabled={matches.length === 0}
        onclick={() => step('prev')}><Icon name="up" /></button
      >
      <button
        class="btn quiet icon"
        aria-label="Next match"
        title="Next (Enter)"
        disabled={matches.length === 0}
        onclick={() => step('next')}><Icon name="down" /></button
      >
      <button class="btn quiet icon" aria-label="Close find" onclick={closeFind}
        ><Icon name="close" /></button
      >
    </div>
  {/if}

  {#if saveError}
    <div class="pad">
      <ErrorNotice error={saveError}>
        {#snippet actions()}
          <button class="btn quiet" onclick={() => (saveError = null)}>Dismiss</button>
        {/snippet}
      </ErrorNotice>
    </div>
  {/if}

  {#if loading}
    <p class="state">Opening <code>{openedPath}</code>…</p>
  {:else if loadError}
    <div class="pad">
      <ErrorNotice error={loadError}>
        {#snippet actions()}
          {#if loadError?.code !== 'binary' && loadError?.code !== 'too-large' && openedPath}
            <button class="btn" onclick={() => load(openedPath)}>Try again</button>
          {/if}
          <button class="btn quiet" onclick={onclosed}>Close tab</button>
        {/snippet}
      </ErrorNotice>
    </div>
  {:else}
    <textarea
      bind:this={ta}
      class="editor"
      class:mono
      class:nowrap={!wrap}
      aria-label="Text of {doc.name}"
      value={doc.text}
      oninput={onInput}
      onselect={trackCursor}
      onkeyup={trackCursor}
      onclick={trackCursor}
      spellcheck="false"
      autocapitalize="off"
      wrap={wrap ? 'soft' : 'off'}></textarea>
  {/if}

  <p class="statusbar">
    <span class="num">Ln {pos.line}, Col {pos.column}</span>
    <span>{doc.eol === '\r\n' ? 'CRLF' : 'LF'}</span>
    <span>UTF-8</span>
    <span class="grow"></span>
    {#if saving}<span>Saving…</span>{:else if dirty}<span class="dirty">Unsaved changes</span
      >{:else if doc.path}<span>Saved</span>{/if}
  </p>

  {#if picker}
    <FilePicker
      mode={picker}
      start={picker === 'save'
        ? saveStart
        : (doc.path && parentPath(doc.path)) || 'home:/Documents'}
      suggestedName={doc.path ? doc.name : `${doc.name}.txt`}
      oncancel={() => {
        picker = null;
        if (asking === 'close') cancelClose();
      }}
      onpick={(path) => {
        if (picker === 'open') {
          picker = null;
          onopen(path);
        } else void savedAs(path);
      }}
    />
  {/if}

  {#if asking === 'close' && !picker}
    <Dialog title="Save changes to {doc.name}?" oncancel={cancelClose}>
      <p>If you don't save, your changes are lost.</p>
      {#snippet actions()}
        <button class="btn danger" onclick={closeNow}>Don't save</button>
        <button class="btn" onclick={cancelClose}>Cancel</button>
        <button class="btn primary" onclick={saveThenClose}>Save</button>
      {/snippet}
    </Dialog>
  {/if}
</div>

<style>
  .notes[hidden] {
    display: none;
  }
  .notes {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .toolbar .btn {
    padding: 0 var(--space-3);
  }
  .toolbar .btn.icon {
    padding: 0;
  }
  .toolbar [aria-pressed='true'] {
    background: var(--selected);
  }
  .grow {
    flex: 1;
  }
  .find {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .find .input {
    flex: 1;
    min-width: 6rem;
    max-width: 22rem;
  }
  .count {
    min-width: 5.5rem;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .case {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    white-space: nowrap;
  }
  .pad {
    padding: var(--space-3) var(--space-3) 0;
  }
  .state {
    flex: 1;
    padding: var(--space-5);
    color: var(--text-muted);
  }
  .editor {
    flex: 1;
    min-height: 0;
    width: 100%;
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border: 0;
    resize: none;
    background: var(--surface);
    font-size: var(--text-md);
    line-height: 1.55;
    tab-size: 4;
    outline: none;
  }
  .editor.mono {
    font-family: var(--font-data);
    font-size: var(--text-sm);
    font-variant-ligatures: none;
  }
  .editor.nowrap {
    white-space: pre;
    overflow-x: auto;
  }
  .editor:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .statusbar {
    display: flex;
    gap: var(--space-4);
    align-items: center;
    min-height: 28px;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .dirty {
    color: var(--accent-ink);
    font-weight: 650;
  }
</style>
