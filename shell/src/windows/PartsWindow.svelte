<script lang="ts">
  import { untrack } from 'svelte';
  import { useCore } from '../lib/state/context';
  import { t } from '../lib/i18n';
  import { portKindLabel } from '../lib/state/portHints';
  import type { ErrorBody, PartInfo, SignalInfo, Simulate } from '../lib/state/types';
  import { canMove } from '../lib/state/driving';
  import {
    isBridge,
    isSubPart,
    linkText,
    localName,
    mayNeedPauseForUpload,
    removable,
  } from '../lib/state/bridges';
  import { isManualSensor } from '../lib/state/simulation';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { menuTrigger, useMenu } from '../components/menuController.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import { openDrive, openViewer as showViewer, viewable } from '../components/driveWindow';
  import AddPartForm from './AddPartForm.svelte';
  import ManualSimulation from './ManualSimulation.svelte';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();

  let adding = $state(false);
  let addPort = $state<string | null>(null);
  let addSimulate = $state<Simulate | null>(null);
  let addConnector = $state<string | null>(null);
  /** A fresh form each time another window asks for one, even while one is open. */
  let formKey = $state(0);

  // Opened to add something particular: from a recording's Properties ("Add a part on this
  // recording": `addPort`), from the World window ("Add a simulated base": `addSimulate`
  // and `addConnector`), or after installing a package (`addConnector`). Start the form there.
  $effect(() => {
    const { addPort: port, addSimulate: simulate, addConnector: connector } = win.params;
    if (!port && !simulate && !connector) return;
    addPort = port ?? null;
    addSimulate = simulate === 'world' || simulate === 'manual' ? simulate : null;
    addConnector = connector ?? null;
    untrack(() => formKey++);
    adding = true;
    desktop.update(win.id, { params: {} });
  });
  let confirming = $state<string | null>(null);
  let removing = $state<string | null>(null);
  let removeError = $state<{ part: string; error: ErrorBody } | null>(null);
  /** A pause or resume the core refused, shown on the part's row. */
  let pauseError = $state<{ part: string; error: ErrorBody } | null>(null);
  let pausing = $state<string | null>(null);
  let justAdded = $state<string | null>(null);

  const parts = $derived(core.parts);

  function connectorName(id: string): string {
    return core.connectors?.find((c) => c.id === id)?.name ?? id;
  }

  function portLabel(id: string | undefined): string | null {
    if (!id) return null;
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
    const paused = p.state.kind === 'paused';
    // A paused part draws nothing and can't be driven until it is resumed.
    const signals = paused ? [] : viewable(p);
    const port = core.ports?.find((x) => x.id === p.port);
    const busy = p.simulate
      ? t('parts.menu.simulatedNoPort')
      : port?.claimedBy === p.name
        ? t('parts.menu.busy.self', { part: p.name })
        : port?.claimedBy
          ? t('parts.menu.busy.other', { owner: port.claimedBy })
          : !port
            ? t('parts.menu.busy.missing', { port: p.port ?? '' })
            : undefined;
    // A board's part has its board's port, which the board uses: no serial monitor for it.
    const sub = isSubPart(p);
    return {
      label: p.name,
      entries: [
        !paused &&
          item('view', t('parts.menu.view'), {
            disabledReason: signals.length ? undefined : t('parts.menu.noSignal'),
          }),
        canMove(p) && !paused && item('drive', t('parts.menu.drive'), { icon: 'drive' }),
        !sub &&
          item(
            'monitor',
            port
              ? t('parts.menu.monitorKind', { kind: portKindLabel(port).toLowerCase() })
              : t('parts.menu.monitor'),
            {
              disabledReason: busy,
            },
          ),
        !sub && !paused && item('pause', t('parts.menu.pause')),
        !sub && paused && item('resume', t('parts.menu.resume')),
        removable(p) && separator(),
        removable(p) && item('remove', t('parts.menu.remove'), { danger: true }),
      ],
      onSelect: (id: string) => {
        if (id === 'pause' || id === 'resume') void setPaused(p.name, id === 'pause');
        else if (id === 'view' && signals[0]) openViewer(p, signals[0]);
        else if (id === 'drive') openDrive(desktop, p.name);
        else if (id === 'monitor' && p.port) openInMonitor(p.port);
        else if (id === 'remove') confirming = p.name;
      },
    };
  }

  /** Pause or resume; the parts topic then shows the new state. */
  async function setPaused(name: string, paused: boolean) {
    pausing = name;
    pauseError = null;
    const r = paused ? await core.pausePart(name) : await core.resumePart(name);
    pausing = null;
    if (!r.ok) pauseError = { part: name, error: r.error };
  }

  function openViewer(p: PartInfo, s: SignalInfo) {
    showViewer(desktop, p, s);
  }

  const VIEWER_ICONS = {
    'laser-scan@1': 'scan',
    'distance@1': 'distance',
    'planar-base@1': 'base',
  } as const;

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

  function closeForm() {
    adding = false;
    addPort = addSimulate = addConnector = null;
  }

  function added(name: string) {
    closeForm();
    justAdded = name;
  }
</script>

{#if adding}
  {#key formKey}
    <AddPartForm
      initialPort={addPort}
      initialSimulate={addSimulate}
      initialConnector={addConnector}
      onadded={added}
      oncancel={closeForm}
    />
  {/key}
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
          {@const paused = p.state.kind === 'paused'}
          {@const signals = paused ? [] : viewable(p)}
          {@const label = portLabel(p.port)}
          {@const sub = isSubPart(p)}
          <li
            class="part"
            class:sub
            class:fresh={justAdded === p.name}
            data-part={p.name}
            data-bridge={p.bridge}
            tabindex="-1"
          >
            <div class="main">
              <div class="title-row">
                {#snippet partName()}{#if sub}<span class="prefix">{p.bridge}.</span>{localName(
                      p,
                    )}{:else}{p.name}{/if}{/snippet}
                {#if signals[0]}
                  <button
                    class="name link"
                    onclick={() => openViewer(p, signals[0]!)}
                    title={t('parts.list.openViewer')}
                  >
                    {@render partName()}
                  </button>
                {:else}
                  <span class="name">{@render partName()}</span>
                {/if}
                <StateBadge kind={p.state.kind} />
                {#if p.simulate}<span class="badge">{t('parts.list.simulatedBadge')}</span>{/if}
                {#if p.board?.hardwareEstop}<span class="badge danger"
                    >{t('parts.board.hardwareEstop')}</span
                  >{/if}
              </div>
              <p class="meta">
                {#if sub}
                  <span>{t('parts.list.onBoard', { bridge: p.bridge ?? '' })}</span>
                  {#each p.signals as s (s.address)}
                    <span class="sep" aria-hidden="true">·</span>
                    <code>{s.capability}</code>
                  {/each}
                {:else}
                  <span>{connectorName(p.connector)}</span>
                  <span class="sep" aria-hidden="true">·</span>
                  {#if p.simulate}
                    <span class="sim">{t(`parts.list.simulated.${p.simulate}`)}</span>
                  {:else}
                    <span>{label ?? t('parts.list.port')} <code>{p.port}</code></span>
                  {/if}
                {/if}
                {#if canMove(p) && !paused}
                  {@const lease = core.leaseOn(p.name)}
                  <span class="sep" aria-hidden="true">·</span>
                  <span class:driven={lease}
                    >{lease
                      ? t('parts.list.drivenBy', { holder: lease.holder })
                      : t('parts.list.notDriven')}</span
                  >
                {/if}
              </p>
              {#if isBridge(p) && p.board}
                {@const b = p.board}
                <p class="meta board">
                  <span class="link-state" data-link={b.link}>{linkText(b.link)}</span>
                  {#if b.board}
                    <span class="sep" aria-hidden="true">·</span>
                    <span
                      >{t('parts.board.firmware', {
                        board: b.board,
                        firmware: b.firmware ?? '',
                        version: b.firmwareVersion ?? '',
                      })}</span
                    >
                  {/if}
                  {#if b.protocol !== undefined}
                    <span class="sep" aria-hidden="true">·</span>
                    <span>{t('parts.board.protocol', { version: b.protocol })}</span>
                  {/if}
                </p>
                {#if b.hardwareEstop}
                  <p class="hint estop-hint">{t('parts.board.hardwareEstop.hint')}</p>
                {/if}
              {/if}
              {#if paused}
                <p class="hint paused-hint">
                  {sub ? t('parts.list.paused.onBoard') : t('parts.list.paused.hint')}
                </p>
              {:else if mayNeedPauseForUpload(p)}
                <p class="hint upload-hint">
                  {t('parts.board.uploadHint')}
                  <button
                    class="btn"
                    disabled={pausing === p.name}
                    aria-label={t('parts.list.pauseNamed', { name: p.name })}
                    onclick={() => setPaused(p.name, true)}>{t('parts.list.pause')}</button
                  >
                </p>
              {/if}
              {#if pauseError?.part === p.name}
                <ErrorNotice error={pauseError.error} />
              {/if}
              {#if isManualSensor(p) && !paused}<ManualSimulation part={p} />{/if}
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
                {#if paused && !sub}
                  <button
                    class="btn primary"
                    disabled={pausing === p.name}
                    aria-label={t('parts.list.resumeNamed', { name: p.name })}
                    onclick={() => setPaused(p.name, false)}
                  >
                    {t('parts.list.resume')}
                  </button>
                {/if}
                {#if canMove(p) && !paused}
                  <button
                    class="btn"
                    onclick={() => openDrive(desktop, p.name)}
                    aria-label={t('parts.list.driveNamed', { name: p.name })}
                  >
                    <Icon name="drive" size={17} />
                    {t('parts.list.drive')}
                  </button>
                {/if}
                {#each signals as s (s.address)}
                  <button
                    class="btn"
                    onclick={() => openViewer(p, s)}
                    aria-label={t('parts.list.viewFor', { address: s.address })}
                  >
                    <Icon
                      name={VIEWER_ICONS[s.capability as keyof typeof VIEWER_ICONS] ?? 'scan'}
                      size={17}
                    />
                    {t('parts.list.view')}
                  </button>
                {/each}
                {#if removable(p)}
                  <button
                    class="btn quiet icon"
                    aria-label={t('parts.list.removeNamed', { name: p.name })}
                    title={t('parts.list.removeNamed', { name: p.name })}
                    onclick={() => (confirming = p.name)}
                  >
                    <Icon name="trash" size={18} />
                  </button>
                {/if}
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
  /* A board's parts sit under the board, indented, joined to it by a line. */
  .part.sub {
    padding-left: calc(var(--space-4) + var(--space-6));
    border-bottom-color: var(--line);
    background: var(--surface-2);
    position: relative;
  }
  .part.sub::before {
    content: '';
    position: absolute;
    left: calc(var(--space-4) + var(--space-2));
    top: 0;
    bottom: 50%;
    width: var(--space-3);
    border-left: 2px solid var(--line-strong);
    border-bottom: 2px solid var(--line-strong);
    border-bottom-left-radius: var(--radius-sm);
  }
  .prefix {
    color: var(--text-muted);
    font-weight: 500;
  }
  .badge.danger {
    background: var(--danger-soft);
    color: var(--danger-text);
  }
  .link-state {
    font-weight: 600;
  }
  .link-state[data-link='ready'] {
    color: var(--ok-text);
  }
  .estop-hint {
    font-size: var(--text-sm);
    color: var(--danger-text);
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
  .badge {
    padding: 1px 8px;
    border-radius: var(--radius-sm);
    background: var(--info-soft);
    color: var(--info-text);
    font-size: var(--text-xs);
    font-weight: 700;
  }
  .driven {
    color: var(--text);
    font-weight: 600;
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
