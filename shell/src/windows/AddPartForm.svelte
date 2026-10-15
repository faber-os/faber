<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { validatePartName } from '../lib/state/partName';
  import { portAvailability, portHints } from '../lib/state/portHints';
  import type { ErrorBody } from '../lib/state/types';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';

  let {
    onadded,
    oncancel,
    initialPort = null,
  }: {
    onadded: (name: string) => void;
    oncancel: () => void;
    initialPort?: string | null;
  } = $props();

  const core = useCore();

  let port = $state<string | null>(null);
  let connector = $state<string | null>(null);
  let connectorTouched = $state(false);
  let name = $state('');
  let nameTouched = $state(false);
  let submitting = $state(false);
  let error = $state<ErrorBody | null>(null);

  const ports = $derived(core.ports);
  const connectors = $derived(core.connectors);
  const nameProblem = $derived(validatePartName(name));
  const showNameProblem = $derived(nameTouched && nameProblem !== null);
  const errorField = $derived(fieldFor(error?.code));

  function fieldFor(code: string | undefined): 'name' | 'port' | 'connector' | null {
    if (code === 'invalid-name' || code === 'name-taken') return 'name';
    if (code === 'port-claimed' || code === 'unknown-port') return 'port';
    if (code === 'unknown-connector') return 'connector';
    return null;
  }

  function pickPort(id: string) {
    port = id;
    if (errorField === 'port') error = null;
    // Detect, then suggest: pre-pick the connector the core thinks fits, unless one was chosen.
    const suggestion = ports?.find((p) => p.id === id)?.suggestions[0];
    if (!connectorTouched && suggestion && connectors?.some((c) => c.id === suggestion.connector)) {
      connector = suggestion.connector;
    }
  }

  // Started from a recording: pick its port (and the suggested connector) once both lists are in.
  let started = false;
  $effect(() => {
    if (started || !initialPort || !ports || !connectors) return;
    started = true;
    if (ports.some((p) => p.id === initialPort)) pickPort(initialPort);
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    nameTouched = true;
    if (nameProblem || !port || !connector) return;
    submitting = true;
    error = null;
    const r = await core.addPart({ name, connector, port });
    submitting = false;
    if (r.ok) onadded(r.value.name);
    else error = r.error;
  }
</script>

<form class="add" onsubmit={submit} novalidate aria-label="Add part">
  <div class="toolbar">
    <button type="button" class="btn quiet" onclick={oncancel}>
      <Icon name="back" size={17} /> Parts
    </button>
    <h3>Add part</h3>
  </div>

  <div class="scroll">
    <fieldset class="step" class:flag={errorField === 'port'}>
      <legend><span class="n">1</span> Pick the port it is plugged into</legend>
      <div class="problems"><PortProblems /></div>
      {#if ports === null}
        <p class="muted">Looking for ports…</p>
      {:else if ports.length === 0}
        <p class="muted">
          No ports found. Plug in the USB adapter, then wait a moment: this list updates on its own.
        </p>
      {:else}
        <div class="choices">
          {#each ports as p (p.id)}
            {@const avail = portAvailability(p)}
            {@const hints = portHints(p, connectors ?? [])}
            <label class="choice" class:disabled={!avail.free}>
              <input
                type="radio"
                name="port"
                value={p.id}
                checked={port === p.id}
                disabled={!avail.free}
                onchange={() => pickPort(p.id)}
              />
              <span class="choice-body">
                <span class="choice-title"
                  >{#if p.kind === 'bluetooth'}<span class="kind">Bluetooth</span
                    >{/if}{p.label}</span
                >
                <span class="choice-meta">
                  <code>{p.id}</code>
                  <span class="pill" class:busy={!avail.free}>{avail.text}</span>
                </span>
                {#each hints as h (h.connector)}
                  <span class="suggest" title={h.reason}>{h.text}</span>
                {/each}
              </span>
            </label>
          {/each}
        </div>
      {/if}
    </fieldset>

    <fieldset class="step" class:flag={errorField === 'connector'}>
      <legend><span class="n">2</span> Pick the connector that talks to it</legend>
      {#if core.connectorsError}
        <ErrorNotice error={core.connectorsError}>
          {#snippet actions()}
            <button type="button" class="btn" onclick={() => core.loadConnectors()}
              >Try again</button
            >
          {/snippet}
        </ErrorNotice>
      {:else if connectors === null}
        <p class="muted">Loading connectors…</p>
      {:else}
        <div class="choices">
          {#each connectors as c (c.id)}
            <label class="choice">
              <input
                type="radio"
                name="connector"
                value={c.id}
                checked={connector === c.id}
                onchange={() => {
                  connector = c.id;
                  connectorTouched = true;
                  if (errorField === 'connector') error = null;
                }}
              />
              <span class="choice-body">
                <span class="choice-title">{c.name}</span>
                <span class="choice-meta">
                  <code>{c.id}</code>
                  <span>v{c.version}</span>
                  <span>serial, {c.port.baud} baud</span>
                </span>
                <span class="provides">
                  Provides {c.provides.map((p) => p.capability).join(', ')}
                </span>
              </span>
            </label>
          {/each}
        </div>
      {/if}
    </fieldset>

    <fieldset class="step" class:flag={errorField === 'name'}>
      <legend><span class="n">3</span> Give it a name</legend>
      <div class="field">
        <label class="label" for="part-name">Name</label>
        <input
          id="part-name"
          class="input mono"
          bind:value={name}
          onblur={() => (nameTouched = true)}
          oninput={() => {
            if (errorField === 'name') error = null;
          }}
          aria-invalid={showNameProblem || errorField === 'name'}
          aria-describedby="part-name-help"
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          placeholder="front_lidar"
        />
        <p id="part-name-help" class="help" class:bad={showNameProblem}>
          {#if showNameProblem}
            {nameProblem?.message} {nameProblem?.hint}
          {:else}
            Lowercase letters, digits and _, starting with a letter. Apps use it to find the part.
          {/if}
        </p>
      </div>
    </fieldset>
  </div>

  {#if error}
    <!-- Outside the scrolling area, so the answer is visible next to the button that caused it. -->
    <div class="answer"><ErrorNotice {error} /></div>
  {/if}

  <div class="footer">
    <p class="muted small">
      {#if !port}Pick a port to continue.{:else if !connector}Pick a connector to continue.{:else}Adds
        the part to <code>parts.toml</code> and starts it.{/if}
    </p>
    <button type="button" class="btn quiet" onclick={oncancel}>Cancel</button>
    <button type="submit" class="btn primary" disabled={submitting || !port || !connector}>
      {submitting ? 'Adding…' : 'Add part'}
    </button>
  </div>
</form>

<style>
  .add {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .toolbar h3 {
    font-size: var(--text-md);
    font-weight: 650;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: var(--space-5);
    padding: var(--space-4) var(--space-4) var(--space-5);
  }
  .step {
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  legend {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
    padding: 0;
    font-weight: 650;
  }
  .n {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--surface-3);
    font-size: var(--text-xs);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .flag .n {
    background: var(--danger);
    color: var(--surface);
  }
  .problems:not(:empty) {
    display: grid;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }
  .choices {
    display: grid;
    gap: var(--space-2);
  }
  .choice {
    position: relative;
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    min-height: 52px;
    padding: var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface);
    cursor: pointer;
    transition:
      border-color var(--dur-fast),
      background var(--dur-fast);
  }
  .choice:hover:not(.disabled) {
    background: var(--surface-2);
  }
  .choice:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .choice:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .choice.disabled {
    cursor: not-allowed;
    background: var(--surface-2);
  }
  .choice.disabled .choice-title {
    color: var(--text-muted);
  }
  .choice input {
    margin: 3px 0 0;
    accent-color: var(--accent);
    width: 18px;
    height: 18px;
    flex: none;
  }
  .choice-body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .choice-title {
    font-weight: 620;
  }
  .choice-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .pill {
    font-size: var(--text-xs);
    font-weight: 650;
    color: var(--ok);
  }
  .pill.busy {
    color: var(--warn);
  }
  .suggest {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--accent-ink);
  }
  .kind {
    margin-right: var(--space-2);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--info-soft);
    color: var(--info);
    font-size: var(--text-xs);
    font-weight: 700;
  }
  .provides {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .help {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .help.bad {
    color: var(--danger);
    font-weight: 560;
  }
  .muted {
    color: var(--text-muted);
  }
  .small {
    font-size: var(--text-sm);
  }
  .answer {
    padding: var(--space-3) var(--space-4) 0;
    border-top: 1px solid var(--line);
  }
  .footer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
  }
  .footer p {
    flex: 1;
    min-width: 0;
  }
  #part-name {
    max-width: 22rem;
  }
</style>
