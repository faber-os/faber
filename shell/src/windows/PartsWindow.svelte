<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { portKindLabel } from '../lib/state/portHints';
  import type { ErrorBody, PartInfo, SignalInfo } from '../lib/state/types';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { menuTrigger, useMenu } from '../components/menuController.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
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
    return port.kind === 'bluetooth' ? `Bluetooth: ${port.label}` : port.label;
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
        ? `"${p.name}" is using this port, and a port can only be open once. Remove the part to talk to the port directly.`
        : port?.claimedBy
          ? `The port is in use by ${port.claimedBy}.`
          : !port
            ? `${p.port} isn't connected right now.`
            : undefined;
    return {
      label: p.name,
      entries: [
        item('view', 'Open viewer', {
          disabledReason: signals.length
            ? undefined
            : 'This part has no signal Faber can draw yet.',
        }),
        item(
          'monitor',
          `Open ${port ? portKindLabel(port).toLowerCase() + ' port' : 'port'} in serial monitor`,
          {
            disabledReason: busy,
          },
        ),
        separator(),
        item('remove', 'Remove', { danger: true }),
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
      title: `${p.name} · scan`,
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
      title: 'Recordings · Files',
      params: { path: 'home:/Recordings' },
    });
  }

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
          Loading parts…
        {:else}
          <span class="num">{parts.length}</span>
          {parts.length === 1 ? 'part' : 'parts'}
          <span class="file">from <code>parts.toml</code></span>
        {/if}
      </p>
      <button
        class="btn primary"
        onclick={() => (adding = true)}
        disabled={core.connection !== 'connected'}
      >
        <Icon name="plus" size={17} /> Add part
      </button>
    </div>

    {#if core.connection === 'reconnecting'}
      <div class="pad">
        <ErrorNotice
          tone="warn"
          error={{
            message: 'Not connected to the core, so the states below may be out of date.',
            hint: 'Faber reconnects on its own. If this stays, check that faber-core is running.',
          }}
        />
      </div>
    {/if}

    {#if parts !== null && parts.length === 0}
      <div class="empty">
        <h3>No parts yet</h3>
        <PortProblems />
        <p>
          A part is one piece of hardware you add, like a LiDAR on a USB adapter. Plug it in: its
          port appears here by itself, and Faber suggests what it might be.
        </p>
        <p>
          No hardware yet? Open <button class="inline-link" onclick={() => openRecordings()}
            >Files → Recordings</button
          >
          and choose <em>Add sample recordings</em>. The sample then shows up as a port you can add
          a part on.
        </p>
        <button
          class="btn primary"
          onclick={() => (adding = true)}
          disabled={core.connection !== 'connected'}
        >
          <Icon name="plus" size={17} /> Add the first part
        </button>
      </div>
    {:else if parts !== null}
      <ul class="list" aria-label="Parts" use:menuTrigger={{ menu, build: partMenu }}>
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
                    title="Open the viewer"
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
                <span>{label ?? 'Port'} <code>{p.port}</code></span>
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
                <span class="confirm-text">Remove from parts.toml?</span>
                <button
                  class="btn danger solid"
                  disabled={removing === p.name}
                  onclick={() => remove(p.name)}
                >
                  {removing === p.name ? 'Removing…' : 'Remove'}
                </button>
                <button class="btn quiet" onclick={() => (confirming = null)}>Keep</button>
              {:else}
                {#each signals as s (s.address)}
                  <button
                    class="btn"
                    onclick={() => openViewer(p, s)}
                    aria-label="Open the viewer for {s.address}"
                  >
                    <Icon name="scan" size={17} /> View
                  </button>
                {/each}
                <button
                  class="btn quiet icon"
                  aria-label="Remove {p.name}"
                  title="Remove {p.name}"
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
  .empty {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    max-width: 52ch;
    padding: var(--space-6) var(--space-5);
  }
  .empty h3 {
    font-size: var(--text-xl);
    font-weight: 680;
    letter-spacing: -0.015em;
  }
  .empty p {
    color: var(--text-muted);
  }
  .empty em {
    font-style: normal;
    font-weight: 650;
    color: var(--text);
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
