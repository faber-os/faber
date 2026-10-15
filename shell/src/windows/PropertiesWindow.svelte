<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { parentPath, replayPortFor } from '../lib/desktop/filePaths';
  import { useDesktop } from '../components/desktopController.svelte';
  import { download, openFolder, openInNotes } from '../components/fileOps';
  import { entryIcon } from '../components/fileLook';
  import { formatBytes } from '../components/format';
  import { formatDateTime } from '../lib/desktop/wallClock';
  import Icon from '../components/Icon.svelte';
  import RichText from '../components/RichText.svelte';
  import { t } from '../lib/i18n';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();

  const p = $derived(win.params);
  const isFile = $derived(p.kind === 'file');
  const replay = $derived(isFile ? replayPortFor(p.path ?? '') : null);
  const modified = $derived(
    p.modifiedMs ? formatDateTime(Number(p.modifiedMs), core.wallClock) : '',
  );
  const where = $derived(p.path ? parentPath(p.path) : null);

  function addPart() {
    if (!replay) return;
    const id = desktop.open('parts');
    desktop.update(id, { params: { addPort: replay } });
  }
</script>

<div class="props">
  <div class="head">
    <span class="ico"
      ><Icon
        name={entryIcon({
          kind: isFile ? 'file' : 'folder',
          name: p.name ?? '',
          special: p.special as never,
        })}
        size={36}
      /></span
    >
    <h3 class="selectable">{p.name}</h3>
  </div>

  {#if replay}
    <div class="replay">
      <p><RichText text={t('properties.recording.intro')} /></p>
      <p><RichText text={t('properties.recording.port', { port: replay })} /></p>
      <button class="btn primary" onclick={addPart}>{t('properties.recording.addPart')}</button>
    </div>
  {/if}

  <dl>
    <dt>{t('properties.kind')}</dt>
    <dd>
      {p.special
        ? t('properties.kind.fixedFolder')
        : isFile
          ? t('properties.kind.file')
          : t('properties.kind.folder')}
    </dd>
    {#if isFile}
      <dt>{t('properties.size')}</dt>
      <dd class="num">
        {t('properties.sizeValue', { size: formatBytes(Number(p.size ?? 0)), bytes: p.size ?? '' })}
      </dd>
    {/if}
    {#if modified}
      <dt>{t('properties.modified')}</dt>
      <dd class="num">{modified}</dd>
    {/if}
    <dt>{t('properties.path')}</dt>
    <dd><code class="selectable">{p.path}</code></dd>
    {#if p.originalPath}
      <dt>{t('properties.cameFrom')}</dt>
      <dd><code class="selectable">{p.originalPath}</code></dd>
    {/if}
  </dl>

  <div class="actions">
    {#if where}
      <button class="btn" onclick={() => openFolder(desktop, where)}>
        <Icon name="folder" size={17} />
        {t('properties.showInFiles')}
      </button>
    {/if}
    {#if isFile && !replay}
      <button class="btn" onclick={() => openInNotes(desktop, p.path!)}
        >{t('properties.openWithNotes')}</button
      >
    {/if}
    {#if isFile}
      <button class="btn" onclick={() => download(core, p.path!)}>
        <Icon name="download" size={17} />
        {t('properties.download')}
      </button>
    {/if}
  </div>
</div>

<style>
  .props {
    display: grid;
    align-content: start;
    gap: var(--space-4);
    flex: 1;
    overflow-y: auto;
    padding: var(--space-5);
    background: var(--surface);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .ico {
    color: var(--accent-ink);
  }
  h3 {
    font-size: var(--text-xl);
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .replay {
    display: grid;
    gap: var(--space-2);
    justify-items: start;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    user-select: text;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-4);
    margin: 0;
    font-size: var(--text-sm);
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
