<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { t } from '../lib/i18n';
  import { portKindLabel } from '../lib/state/portHints';
  import type { ErrorBody, PartInfo, SignalInfo } from '../lib/state/types';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { menuTrigger, useMenu } from '../components/menuController.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import AddPartForm from './AddPartForm.svelte';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();

  let adding = $state(false);
  let addPort = $state<string | null>(null);

  // Opened from a recording's Properties ("Add a part on this recording"): start the form there.
  $effect(() => {
    const port = win.params.addPort;
    if (!port) return;
    addPort = port;
    adding = true;
    desktop.update(win.id, { params: {} });
  });
  let confirming = $state<string | null>(null);
  let removing = $state<string | null>(null);
  let removeError = $state<{ part: string; error: ErrorBody } | null>(null);
  let justAdded = $state<string | null>(null);

  const parts = $derived(core.parts);

  function connectorName(id: string): string {
    return core.connectors?.find((c) => c.id === id)?.name ?? id;
  }

  function portLabel(id: string): string | null {
    const port = core.ports?.find((p) => p.id === id);
    if (!port) return null;
    return port.kind === 'bluetooth'
      ? t('parts.list.bluetoothPort', { label: port.label })
      : port.label;
  }

  function openInMonitor(port: string) {
    const id = desktop.open('serial-monitor');
    desktop.update(id, { params: { port } });
  }

  function partMenu(target: HTMLElement) {
    const row = target.closest<HTMLElement>('[data-part]');
    const p = parts?.find((x) => x.name === row?.dataset.part);
    if (!p) return null;
    const signals = viewable(p);
    const port = core.ports?.find((x) => x.id === p.port);
    const busy =
      port?.claimedBy === p.name
        ? t('parts.menu.busy.self', { part: p.name })
        : port?.claimedBy
          ? t('parts.menu.busy.other', { owner: port.claimedBy })
          : !port
            ? t('parts.menu.busy.missing', { port: p.port })
            : undefined;
    return {
      label: p.name,
      entries: [
        item('view', t('parts.menu.view'), {
          disabledReason: signals.length ? undefined : t('parts.menu.noSignal'),
        }),
        item(
          'monitor',
          port
            ? t('parts.menu.monitorKind', { kind: portKindLabel(port).toLowerCase() })
            : t('parts.menu.monitor'),
          {
            disabledReason: busy,
          },
        ),
        separator(),
        item('remove', t('parts.menu.remove'), { danger: true }),
      ],
      onSelect: (id: string) => {
        if (id === 'view' && signals[0]) openViewer(p, signals[0]);
        else if (id === 'monitor') openInMonitor(p.port);
        else if (id === 'remove') confirming = p.name;
      },
    };
  }

  function viewable(p: PartInfo): SignalInfo[] {
    return p.signals.filter((s) => s.capability === 'laser-scan@1');
  }

  function openViewer(p: PartInfo, s: SignalInfo) {
    desktop.open('laser-scan', {
      key: s.address,
      title: t('parts.title.viewer', { name: p.name }),
      params: { address: s.address, part: p.name },
    });
  }

  async function remove(name: string) {
    removing = name;
    removeError = null;
    const r = await core.removePart(name);
    removing = null;
    confirming = null;
    if (!r.ok) removeError = { part: name, error: r.error };
  }

  function openRecordings() {
    desktop.open('files', {
      key: 'files:home:/Recordings',
      title: t('parts.title.recordings'),
      params: { path: 'home:/Recordings' },
    });
  }

  /** `text` split around the first `mark`, so a styled piece can sit inside a sentence. */
  function around(text: string, mark: string): [string, string] {
    const i = text.indexOf(mark);
    return i < 0 ? [text, ''] : [text.slice(0, i), text.slice(i + mark.length)];
  }

  // Stands in for the button while the sentence is split around it.
  const LINK = '\u0000';
  const noHardware = $derived(around(t('parts.empty.noHardware', { link: LINK }), LINK));

  function added(name: string) {
    adding = false;
    addPort = null;
    justAdded = name;
  }
</script>

{#if adding}
  <AddPartForm
    initialPort={addPort}
    onadded={added}
    oncancel={() => {
      adding = false;
      addPort = null;
    }}
  />
{:else}
  <div class="parts">
    <div class="toolbar">
      <p class="summary">
        {#if parts === null}
          {t('parts.loading')}
        {:else}
          {@const [before, after] = around(
            t('parts.summary.count', { count: parts.length }),
            String(parts.length),
          )}
          {before}<span class="num">{parts.length}</span>{after}
          <span class="file"><RichText text={t('parts.summary.file')} /></span>
        {/if}
      </p>
      <button
        class="btn primary"
        onclick={() => (adding = true)}
        disabled={core.connection !== 'connected'}
      >
        <Icon name="plus" size={17} />
        {t('parts.toolbar.add')}
      </button>
    </div>

    {#if core.connection === 'reconnecting'}
      <div class="pad">
        <ErrorNotice
          tone="warn"
          error={{
            message: t('parts.offline.message'),
            hint: t('parts.offline.hint'),
          }}
        />
      </div>
    {/if}

    {#if parts !== null && parts.length === 0}
      <div class="empty">
        <h3>{t('parts.empty.title')}</h3>
        <PortProblems />
        <p>{t('parts.empty.body')}</p>
        <p>
          {noHardware[0]}<button class="inline-link" onclick={() => openRecordings()}
            >{t('parts.empty.sample')}</button
          >{noHardware[1]}
        </p>
        <button
          class="btn primary"
          onclick={() => (adding = true)}
          disabled={core.connection !== 'connected'}
        >
          <Icon name="plus" size={17} />
          {t('parts.empty.addFirst')}
        </button>
      </div>
    {:else if parts !== null}
      <ul
        class="list"
        aria-label={t('parts.list.label')}
        use:menuTrigger={{ menu, build: partMenu }}
      >
        {#each parts as p (p.name)}
          {@const signals = viewable(p)}
          {@const label = portLabel(p.port)}
          <li class="part" class:fresh={justAdded === p.name} data-part={p.name} tabindex="-1">
            <div class="main">
              <div class="title-row">
                {#if signals[0]}
                  <button
                    class="name link"
                    onclick={() => openViewer(p, signals[0]!)}
                    title={t('parts.list.openViewer')}
                  >
                    {p.name}
                  </button>
                {:else}
                  <span class="name">{p.name}</span>
                {/if}
                <StateBadge kind={p.state.kind} />
              </div>
              <p class="meta">
                <span>{connectorName(p.connector)}</span>
                <span class="sep" aria-hidden="true">·</span>
                <span>{label ?? t('parts.list.port')} <code>{p.port}</code></span>
              </p>
              {#if p.state.message}
                <div
                  class="state-msg"
                  class:bad={['failed', 'conflict', 'invalid'].includes(p.state.kind)}
                >
                  <p class="msg">{p.state.message}</p>
                  {#if p.state.hint}<p class="hint">{p.state.hint}</p>{/if}
                </div>
              {/if}
              {#if removeError?.part === p.name}
                <ErrorNotice error={removeError.error} />
              {/if}
            </div>

            <div class="actions">
              {#if confirming === p.name}
                <span class="confirm-text">{t('parts.list.confirmRemove')}</span>
                <button
                  class="btn danger solid"
                  disabled={removing === p.name}
                  onclick={() => remove(p.name)}
                >
                  {removing === p.name ? t('parts.list.removing') : t('parts.list.remove')}
                </button>
                <button class="btn quiet" onclick={() => (confirming = null)}
                  >{t('parts.list.keep')}</button
                >
              {:else}
                {#each signals as s (s.address)}
                  <button
                    class="btn"
                    onclick={() => openViewer(p, s)}
                    aria-label={t('parts.list.viewFor', { address: s.address })}
                  >
                    <Icon name="scan" size={17} />
                    {t('parts.list.view')}
                  </button>
                {/each}
                <button
                  class="btn quiet icon"
                  aria-label={t('parts.list.removeNamed', { name: p.name })}
                  title={t('parts.list.removeNamed', { name: p.name })}
                  onclick={() => (confirming = p.name)}
                >
                  <Icon name="trash" size={18} />
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .parts {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    background: var(--surface);
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .summary {
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .file {
    margin-left: var(--space-1);
    font-weight: 400;
    color: var(--text-muted);
  }
  .pad {
    padding: var(--space-3) var(--space-4) 0;
  }
  /* Scrolls on its own, so nothing is cut off when the window (or the screen) is small. */
  .empty {
    display: grid;
    align-content: start;
    justify-items: start;
    gap: var(--space-3);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-6) var(--space-5);
  }
  .empty > :global(*) {
    max-width: 52ch;
  }
  .empty h3 {
    font-size: var(--text-xl);
    font-weight: 680;
    letter-spacing: -0.015em;
  }
  .empty p {
    color: var(--text-muted);
  }
  .inline-link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font-weight: 650;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .part {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .part.fresh {
    animation: arrive 1.6s var(--ease-out);
  }
  @keyframes arrive {
    from {
      background: var(--accent-soft);
    }
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 3px;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 32px;
  }
  .name {
    font-size: var(--text-lg);
    font-weight: 680;
    letter-spacing: -0.01em;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    text-decoration: underline;
    text-decoration-color: transparent;
    text-underline-offset: 3px;
    transition: text-decoration-color var(--dur-fast);
  }
  .link:hover {
    text-decoration-color: var(--accent);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .meta code {
    color: var(--text);
  }
  .sep {
    color: var(--text-faint);
  }
  .state-msg {
    margin-top: var(--space-1);
    padding-left: var(--space-3);
    border-left: 2px solid var(--line-strong);
    font-size: var(--text-sm);
    user-select: text;
  }
  .state-msg.bad {
    border-left-color: var(--danger);
  }
  .msg {
    font-weight: 600;
  }
  .hint {
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }
  .confirm-text {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  @media (max-width: 560px) {
    .part {
      flex-direction: column;
    }
  }
</style>
