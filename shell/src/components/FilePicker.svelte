<script lang="ts">
  import { childPath } from '../lib/desktop/filePaths';
  import { useCore } from '../lib/state/context';
  import type { FileEntry } from '../lib/state/types';
  import Dialog from './Dialog.svelte';
  import ErrorNotice from './ErrorNotice.svelte';
  import { SPECIALS, entryIcon } from './fileLook';
  import { FolderView } from './folderView.svelte';
  import Icon from './Icon.svelte';

  /**
   * Pick a file to open, or a folder and name to save as. The same browsing as the Files window
   * (FolderView), in a sheet over the window that asked.
   */
  let {
    mode,
    start,
    suggestedName = '',
    onpick,
    oncancel,
  }: {
    mode: 'open' | 'save';
    start: string;
    suggestedName?: string;
    onpick: (path: string) => void;
    oncancel: () => void;
  } = $props();

  const core = useCore();
  // svelte-ignore state_referenced_locally (where the picker starts; it browses on its own after)
  const view = new FolderView(core, start);
  // svelte-ignore state_referenced_locally
  let name = $state(suggestedName);
  let chosen = $state<string | null>(null);
  let lastClick = { path: '', at: 0 };

  $effect(() => view.start());

  const entries = $derived(view.listing?.entries ?? []);
  const taken = $derived(
    mode === 'save' ? entries.find((e) => e.kind === 'file' && e.name === name.trim()) : undefined,
  );
  const target = $derived(
    mode === 'open' ? chosen : name.trim() ? childPath(view.path, name.trim()) : null,
  );

  function click(e: FileEntry) {
    const now = performance.now();
    const twice = lastClick.path === e.path && now - lastClick.at < 450;
    lastClick = twice ? { path: '', at: 0 } : { path: e.path, at: now };
    if (e.kind === 'folder') {
      if (twice || mode === 'save') view.go(e.path);
      chosen = null;
      return;
    }
    if (mode === 'open') chosen = e.path;
    else name = e.name;
    if (twice) pick();
  }

  function pick() {
    if (target) onpick(target);
  }
</script>

<Dialog title={mode === 'open' ? 'Open a file' : 'Save as'} {oncancel} wide>
  <div class="picker">
    <div class="places" role="group" aria-label="Places">
      {#each SPECIALS.filter((s) => s.id !== 'trash') as s (s.id)}
        <button
          class="place"
          class:on={view.path === s.path}
          aria-pressed={view.path === s.path}
          onclick={() => view.go(s.path)}><Icon name={s.icon} size={16} />{s.label}</button
        >
      {/each}
    </div>
    <div class="where">
      <button
        class="btn quiet icon"
        aria-label="Up one folder"
        disabled={!view.canUp}
        onclick={() => view.up()}><Icon name="up" /></button
      >
      <code class="path">{view.path}</code>
    </div>
    {#if view.error}<ErrorNotice error={view.error} />{/if}
    <ul class="entries" aria-label="Files and folders in {view.path}">
      {#each entries as e (e.path)}
        <li>
          <button
            class="entry"
            class:on={chosen === e.path || (mode === 'save' && e.name === name)}
            class:dim={mode === 'save' && e.kind === 'file'}
            onclick={() => click(e)}
          >
            <Icon name={entryIcon(e)} size={17} /><span>{e.name}</span>
          </button>
        </li>
      {:else}
        <li class="none">{view.loading ? 'Loading…' : 'Nothing here.'}</li>
      {/each}
    </ul>
    {#if mode === 'save'}
      <label class="field">
        <span class="label">File name</span>
        <input
          class="input"
          bind:value={name}
          data-autofocus
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              pick();
            }
          }}
        />
      </label>
      {#if taken}
        <p class="warn" role="status">
          There is already a file called “{taken.name}” here. Saving replaces it.
        </p>
      {/if}
    {/if}
  </div>
  {#snippet actions()}
    <button class="btn" onclick={oncancel}>Cancel</button>
    <button class="btn primary" disabled={!target} onclick={pick}>
      {mode === 'open' ? 'Open' : taken ? 'Replace' : 'Save'}
    </button>
  {/snippet}
</Dialog>

<style>
  .picker {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }
  .places {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .place {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    padding: 0 var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface);
    font-size: var(--text-sm);
  }
  .place.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .where {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
  .entries {
    height: min(240px, 40vh);
    overflow-y: auto;
    margin: 0;
    padding: var(--space-1);
    list-style: none;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-sunken);
  }
  .entry {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 36px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    text-align: left;
  }
  .entry span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .entry:hover {
    background: var(--hover);
  }
  .entry.on {
    background: var(--selected);
  }
  .entry.dim {
    color: var(--text-muted);
  }
  .none {
    padding: var(--space-3);
    color: var(--text-muted);
  }
  .warn {
    font-size: var(--text-sm);
    color: var(--warn);
    font-weight: 600;
  }
</style>
