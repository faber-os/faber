<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import type { BackupInfo, ErrorBody, FileEntry, StorageInfo } from '../../lib/state/types';
  import { formatDateTime } from '../../lib/desktop/wallClock';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import { formatBytes } from '../../components/format';
  import { openFolder } from '../../components/fileOps';
  import { useDesktop } from '../../components/desktopController.svelte';

  let { winId }: { winId: string } = $props();

  const BACKUPS = 'home:/Backups';
  /** Above this much of the disk in use, Settings warns (Faber never deletes anything itself). */
  const WARN_PERCENT = 80;

  const core = useCore();
  const desktop = useDesktop();

  let storage = $state<StorageInfo | null>(null);
  let backups = $state<FileEntry[] | null>(null);
  let error = $state<ErrorBody | null>(null);
  let includeRecordings = $state(false);
  let busy = $state<'backup' | 'restore' | null>(null);
  let saved = $state<BackupInfo | null>(null);
  let restored = $state<string | null>(null);
  let confirming = $state<FileEntry | null>(null);

  async function load() {
    const [s, b] = await Promise.all([core.loadStorage(), core.listFolder(BACKUPS)]);
    if (s.ok) storage = s.value;
    else error = s.error;
    if (b.ok) {
      // Newest first: backup names start with the date.
      backups = b.value.entries
        .filter((e) => e.kind === 'file')
        .sort((x, y) => y.name.localeCompare(x.name));
    } else error = b.error;
  }

  // Sizes change with any file, so any change through the core reloads them.
  $effect(() => {
    void load();
    return core.watchFiles(() => void load());
  });

  const used = $derived(storage ? storage.totalBytes - storage.freeBytes : 0);
  const percent = $derived(
    storage && storage.totalBytes > 0 ? Math.round((used / storage.totalBytes) * 100) : 0,
  );
  const segments = $derived.by(() => {
    if (!storage) return [];
    const parts = [
      { label: 'Recordings', bytes: storage.recordingsBytes, colour: 'var(--file-recording)' },
      { label: 'Documents', bytes: storage.documentsBytes, colour: 'var(--app-files)' },
      { label: 'Desktop', bytes: storage.desktopBytes, colour: 'var(--app-logs)' },
      { label: 'Backups', bytes: storage.backupsBytes, colour: 'var(--app-serial)' },
    ];
    const faber = parts.reduce((n, p) => n + p.bytes, 0);
    return [
      ...parts,
      { label: 'Everything else', bytes: Math.max(0, used - faber), colour: 'var(--neutral)' },
    ];
  });

  async function backUp() {
    busy = 'backup';
    error = null;
    saved = null;
    restored = null;
    const r = await core.createBackup(includeRecordings);
    busy = null;
    if (r.ok) saved = r.value;
    else error = r.error;
  }

  async function restore(entry: FileEntry) {
    confirming = null;
    busy = 'restore';
    error = null;
    saved = null;
    restored = null;
    const r = await core.restoreBackup(entry.path);
    busy = null;
    if (r.ok) restored = entry.name;
    else error = r.error;
  }

  const nameOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
</script>

<section id="{winId}-storage" aria-labelledby="{winId}-storage-h">
  <h3 id="{winId}-storage-h">Storage and backups</h3>
  {#if error}<ErrorNotice {error} />{/if}

  <div class="block">
    <p class="name">Disk space</p>
    {#if storage}
      <div
        class="bar"
        role="meter"
        aria-label="Disk space used"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent}
        aria-valuetext="{percent} % used"
      >
        {#each segments as s (s.label)}
          <span
            class="seg"
            style:width="{(s.bytes / storage.totalBytes) * 100}%"
            style:background={s.colour}
          ></span>
        {/each}
      </div>
      <ul class="legend">
        {#each segments as s (s.label)}
          <li>
            <span class="key" style:background={s.colour}></span>{s.label}
            <span class="num">{formatBytes(s.bytes)}</span>
          </li>
        {/each}
        <li>
          <span class="key free"></span>Free
          <span class="num">{formatBytes(storage.freeBytes)}</span>
        </li>
      </ul>
      {#if percent > WARN_PERCENT}
        <ErrorNotice
          tone="warn"
          error={{
            message: `The disk is ${percent} % full.`,
            hint: "Delete recordings or old backups you don't need, then empty the Trash. Faber never deletes anything by itself.",
          }}
        />
      {/if}
    {:else}
      <p class="desc">Measuring…</p>
    {/if}
  </div>

  <div class="row">
    <div class="text">
      <p class="name">Back up this machine</p>
      <p class="desc">
        Saves parts, settings, Desktop and Documents as one .zip in Backups. Download it to keep a
        copy off this machine.
      </p>
      <label class="tick">
        <input type="checkbox" bind:checked={includeRecordings} disabled={busy !== null} />
        Include recordings{storage ? ` (${formatBytes(storage.recordingsBytes)})` : ''}
      </label>
    </div>
    <button class="btn primary" onclick={backUp} disabled={busy !== null}>
      {busy === 'backup' ? 'Backing up…' : 'Back up now'}
    </button>
  </div>
  {#if saved}
    <p class="done" role="status">Saved {nameOf(saved.path)} ({formatBytes(saved.bytes)}).</p>
  {/if}
  {#if restored}
    <p class="done" role="status">Restored {restored}. Any files it replaced are in the Trash.</p>
  {/if}

  <div class="block">
    <div class="list-head">
      <p class="name" id="{winId}-backups-label">Backups</p>
      <button class="btn quiet" onclick={() => openFolder(desktop, BACKUPS)}>Open in Files</button>
    </div>
    {#if backups === null}
      <p class="desc">Loading…</p>
    {:else if backups.length === 0}
      <p class="desc">No backups yet.</p>
    {:else}
      <ul class="backups" aria-labelledby="{winId}-backups-label">
        {#each backups as b (b.path)}
          <li>
            <span class="b-name">{b.name}</span>
            <span class="desc num"
              >{formatBytes(b.size)}{b.modifiedMs
                ? ` · ${formatDateTime(b.modifiedMs, core.wallClock)}`
                : ''}</span
            >
            <a class="btn quiet" href={core.downloadUrl(b.path)} download={b.name}>Download</a>
            <button
              class="btn"
              aria-label="Restore {b.name}"
              disabled={busy !== null}
              onclick={() => (confirming = b)}
              >{busy === 'restore' ? 'Restoring…' : 'Restore…'}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

{#if confirming}
  {@const entry = confirming}
  <Dialog title="Restore this backup?" oncancel={() => (confirming = null)}>
    <p><strong>{entry.name}</strong></p>
    <p>
      Parts and settings become the ones in the backup, and its files are put back. Files it
      replaces go to the Trash, so nothing is lost; files that aren't in the backup stay.
      Whole-machine file access isn't changed.
    </p>
    {#snippet actions()}
      <button class="btn" onclick={() => (confirming = null)}>Cancel</button>
      <button class="btn primary" onclick={() => restore(entry)}>Restore</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .block {
    display: grid;
    gap: var(--space-2);
    padding-bottom: var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .bar {
    display: flex;
    height: 14px;
    border-radius: 7px;
    overflow: hidden;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .seg {
    display: block;
    height: 100%;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .legend .num {
    color: var(--text);
  }
  .key {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }
  .key.free {
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .tick {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-2);
    font-size: var(--text-sm);
  }
  .done {
    font-size: var(--text-sm);
    color: var(--ok);
  }
  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .backups {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }
  .backups li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-height: 48px;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
  }
  .backups li:hover {
    background: var(--hover);
  }
  .b-name {
    flex: 1 1 12rem;
    min-width: 0;
    overflow-wrap: anywhere;
    font-weight: 600;
  }
</style>
