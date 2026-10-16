<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import type { BackupInfo, ErrorBody, FileEntry, StorageInfo } from '../../lib/state/types';
  import { formatDateTime } from '../../lib/desktop/wallClock';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import { formatBytes } from '../../components/format';
  import { openFolder } from '../../components/fileOps';
  import { useDesktop } from '../../components/desktopController.svelte';
  import RoleLock from '../../components/RoleLock.svelte';
  import { t } from '../../lib/i18n';

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
  let busy = $state<'backup' | 'setup' | 'restore' | null>(null);
  let saved = $state<BackupInfo | null>(null);
  let shared = $state<BackupInfo | null>(null);
  let restored = $state<string | null>(null);
  let confirming = $state<FileEntry | null>(null);
  /** Restore the backup's users and API tokens too (docs/auth.md); off: this machine keeps its own. */
  let withUsers = $state(false);
  // Backups hold the users' password hashes: making, listing and restoring them is for admins.
  const lock = $derived(core.auth.lock('admin'));

  async function load() {
    if (lock) {
      const s = await core.loadStorage();
      if (s.ok) storage = s.value;
      else error = s.error;
      return;
    }
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
      {
        label: t('settings.storage.segment.recordings'),
        bytes: storage.recordingsBytes,
        colour: 'var(--file-recording)',
      },
      {
        label: t('settings.storage.segment.documents'),
        bytes: storage.documentsBytes,
        colour: 'var(--app-files)',
      },
      {
        label: t('settings.storage.segment.desktop'),
        bytes: storage.desktopBytes,
        colour: 'var(--app-logs)',
      },
      {
        label: t('settings.storage.segment.backups'),
        bytes: storage.backupsBytes,
        colour: 'var(--app-serial)',
      },
    ];
    const faber = parts.reduce((n, p) => n + p.bytes, 0);
    return [
      ...parts,
      {
        label: t('settings.storage.segment.other'),
        bytes: Math.max(0, used - faber),
        colour: 'var(--neutral)',
      },
    ];
  });

  async function shareSetup() {
    busy = 'setup';
    error = null;
    saved = null;
    shared = null;
    restored = null;
    const r = await core.shareSetup();
    busy = null;
    if (r.ok) shared = r.value;
    else error = r.error;
  }

  /** A setup is named `<machine>-setup-<date>.zip` (a backup `…-backup-…`). */
  const isSetup = (name: string) => /-setup-[^/]*\.zip$/.test(name);

  async function backUp() {
    busy = 'backup';
    error = null;
    saved = null;
    shared = null;
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
    shared = null;
    restored = null;
    const r = await core.restoreBackup(entry.path, withUsers && !isSetup(entry.name));
    withUsers = false;
    busy = null;
    if (r.ok) restored = entry.name;
    else error = r.error;
  }

  const nameOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
</script>

<section id="{winId}-storage" aria-labelledby="{winId}-storage-h">
  <h3 id="{winId}-storage-h">{t('settings.storage.title')}</h3>
  {#if error}<ErrorNotice {error} />{/if}
  <RoleLock reason={lock} />

  <div class="block">
    <p class="name">{t('settings.storage.disk.name')}</p>
    {#if storage}
      <div
        class="bar"
        role="meter"
        aria-label={t('settings.storage.disk.label')}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent}
        aria-valuetext={t('settings.storage.disk.valueText', { percent })}
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
          <span class="key free"></span>{t('settings.storage.segment.free')}
          <span class="num">{formatBytes(storage.freeBytes)}</span>
        </li>
      </ul>
      {#if percent > WARN_PERCENT}
        <ErrorNotice
          tone="warn"
          error={{
            message: t('settings.storage.full.message', { percent }),
            hint: t('settings.storage.full.hint'),
          }}
        />
      {/if}
    {:else}
      <p class="desc">{t('settings.storage.disk.measuring')}</p>
    {/if}
  </div>

  <div class="row">
    <div class="text">
      <p class="name">{t('settings.storage.backup.name')}</p>
      <p class="desc">{t('settings.storage.backup.desc')}</p>
      <label class="tick">
        <input
          type="checkbox"
          bind:checked={includeRecordings}
          disabled={busy !== null || lock !== null}
        />
        {storage
          ? t('settings.storage.backup.recordingsSize', {
              size: formatBytes(storage.recordingsBytes),
            })
          : t('settings.storage.backup.recordings')}
      </label>
    </div>
    <button
      class="btn primary"
      onclick={backUp}
      disabled={busy !== null || lock !== null}
      title={lock ?? undefined}
    >
      {busy === 'backup' ? t('settings.storage.backup.busy') : t('settings.storage.backup.now')}
    </button>
  </div>
  {#if saved}
    <p class="done" role="status">
      {t('settings.storage.backup.saved', {
        name: nameOf(saved.path),
        size: formatBytes(saved.bytes),
      })}
    </p>
  {/if}
  {#if restored}
    <p class="done" role="status">
      {isSetup(restored)
        ? t('settings.storage.setup.restored', { name: restored })
        : t('settings.storage.restore.done', { name: restored })}
    </p>
  {/if}

  <div class="row">
    <div class="text">
      <p class="name">{t('settings.storage.setup.name')}</p>
      <p class="desc">{t('settings.storage.setup.desc')}</p>
    </div>
    <button
      class="btn"
      onclick={shareSetup}
      disabled={busy !== null || lock !== null}
      title={lock ?? undefined}
    >
      {busy === 'setup' ? t('settings.storage.setup.busy') : t('settings.storage.setup.now')}
    </button>
  </div>
  {#if shared}
    <p class="done" role="status">
      {t('settings.storage.setup.saved', {
        name: nameOf(shared.path),
        size: formatBytes(shared.bytes),
      })}
    </p>
  {/if}

  <div class="block">
    <div class="list-head">
      <p class="name" id="{winId}-backups-label">{t('settings.storage.backups.name')}</p>
      <button
        class="btn quiet"
        disabled={lock !== null}
        title={lock ?? undefined}
        onclick={() => openFolder(desktop, BACKUPS)}>{t('settings.storage.backups.open')}</button
      >
    </div>
    {#if lock}
      <p class="desc">{t('settings.storage.backups.locked')}</p>
    {:else if backups === null}
      <p class="desc">{t('settings.storage.backups.loading')}</p>
    {:else if backups.length === 0}
      <p class="desc">{t('settings.storage.backups.empty')}</p>
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
            <a class="btn quiet" href={core.downloadUrl(b.path)} download={b.name}
              >{t('settings.storage.backups.download')}</a
            >
            <button
              class="btn"
              aria-label={t('settings.storage.backups.restoreLabel', { name: b.name })}
              disabled={busy !== null}
              onclick={() => (confirming = b)}
              >{busy === 'restore'
                ? t('settings.storage.backups.restoring')
                : t('settings.storage.backups.restore')}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

{#if confirming}
  {@const entry = confirming}
  <Dialog
    title={isSetup(entry.name)
      ? t('settings.storage.setup.restoreTitle')
      : t('settings.storage.restore.title')}
    oncancel={() => (confirming = null)}
  >
    <p><strong>{entry.name}</strong></p>
    <p>
      {isSetup(entry.name)
        ? t('settings.storage.setup.restoreBody')
        : t('settings.storage.restore.body')}
    </p>
    {#if !isSetup(entry.name)}
      <label class="tick">
        <input type="checkbox" bind:checked={withUsers} />
        {t('settings.storage.restore.users')}
      </label>
      <p class="desc">
        {withUsers
          ? t('settings.storage.restore.users.on')
          : t('settings.storage.restore.users.off')}
      </p>
    {/if}
    {#snippet actions()}
      <button class="btn" onclick={() => (confirming = null)}
        >{t('settings.storage.restore.cancel')}</button
      >
      <button class="btn primary" onclick={() => restore(entry)}
        >{t('settings.storage.restore.confirm')}</button
      >
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
    color: var(--ok-text);
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
