<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { t } from '../lib/i18n';
  import { validatePartName } from '../lib/state/partName';
  import { portAvailability, portHints } from '../lib/state/portHints';
  import {
    defaultValues,
    readSettings,
    settingFields,
    type SettingValues,
  } from '../lib/state/connectorSettings';
  import type { ErrorBody, NewPart, Simulate } from '../lib/state/types';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
  import RichText from '../components/RichText.svelte';

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
  /** The Simulated switch as the user left it; a generic connector turns it on regardless. */
  let simulatedChoice = $state(false);
  let mode = $state<Simulate>('manual');
  let values = $state<SettingValues>({});
  let settingsProblem = $state<{ key: string; message: string } | null>(null);

  const ports = $derived(core.ports);
  const connectors = $derived(core.connectors);
  const nameProblem = $derived(validatePartName(name));
  const showNameProblem = $derived(nameTouched && nameProblem !== null);
  const errorField = $derived(fieldFor(error?.code));
  const chosen = $derived(connectors?.find((c) => c.id === connector) ?? null);
  /** Generic connectors need a FaberLink bridge to be real (Phase 4): simulated only for now. */
  const needsBridge = $derived(chosen?.port.kind === 'bridge');
  const simulated = $derived(simulatedChoice || needsBridge);
  const fields = $derived(settingFields(connector));
  const modes = $derived([
    {
      id: 'manual' as Simulate,
      title: t('addPart.source.manual'),
      help: t('addPart.source.manual.help'),
    },
    {
      id: 'world' as Simulate,
      title: t('addPart.source.world'),
      help: t('addPart.source.world.help'),
    },
  ]);

  function fieldFor(code: string | undefined): 'name' | 'port' | 'connector' | 'settings' | null {
    if (code === 'invalid-name' || code === 'name-taken') return 'name';
    if (code === 'port-claimed' || code === 'unknown-port') return 'port';
    // Where the values come from (step 1): a port, or simulated.
    if (code === 'needs-bridge' || code === 'port-and-simulate' || code === 'world-has-a-base')
      return 'port';
    if (code === 'unknown-connector') return 'connector';
    if (code === 'invalid-settings') return 'settings';
    return null;
  }

  function chooseConnector(id: string) {
    connector = id;
    values = defaultValues(settingFields(id));
    settingsProblem = null;
  }

  function pickPort(id: string) {
    port = id;
    if (errorField === 'port') error = null;
    // Detect, then suggest: pre-pick the connector the core thinks fits, unless one was chosen.
    const suggestion = ports?.find((p) => p.id === id)?.suggestions[0];
    if (!connectorTouched && suggestion && connectors?.some((c) => c.id === suggestion.connector)) {
      chooseConnector(suggestion.connector);
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
    if (nameProblem || !connector || (!simulated && !port)) return;
    const part: NewPart = simulated
      ? { name, connector, simulate: mode }
      : { name, connector, port: port ?? undefined };
    if (fields.length > 0) {
      const read = readSettings(fields, values);
      if (!read.ok) {
        settingsProblem = read;
        return;
      }
      part.settings = read.settings;
    }
    submitting = true;
    error = null;
    const r = await core.addPart(part);
    submitting = false;
    if (r.ok) onadded(r.value.name);
    else error = r.error;
  }
</script>

<form class="add" onsubmit={submit} novalidate aria-label={t('addPart.form.label')}>
  <div class="toolbar">
    <button type="button" class="btn quiet" onclick={oncancel}>
      <Icon name="back" size={17} />
      {t('addPart.back')}
    </button>
    <h3>{t('addPart.title')}</h3>
  </div>

  <div class="scroll">
    <fieldset class="step" class:flag={errorField === 'port'}>
      <legend
        ><span class="n">1</span>
        {simulated ? t('addPart.source.mode') : t('addPart.port.legend')}</legend
      >
      <!-- The help sits outside the label, so the switch is just called "Simulated". -->
      <div class="switch-row" class:locked={needsBridge}>
        <input
          id="simulated"
          type="checkbox"
          role="switch"
          checked={simulated}
          disabled={needsBridge}
          aria-describedby="simulated-help"
          onchange={(e) => {
            simulatedChoice = e.currentTarget.checked;
            if (errorField === 'port') error = null;
          }}
        />
        <span class="choice-body">
          <label class="choice-title" for="simulated">{t('addPart.source.simulated')}</label>
          <span class="help" id="simulated-help">
            {needsBridge && chosen
              ? t('addPart.source.locked', { connector: chosen.name })
              : t('addPart.source.simulated.help')}
          </span>
        </span>
      </div>
      {#if simulated}
        <div class="choices modes">
          {#each modes as m (m.id)}
            <label class="choice">
              <input
                type="radio"
                name="simulate"
                value={m.id}
                checked={mode === m.id}
                onchange={() => (mode = m.id)}
              />
              <span class="choice-body">
                <span class="choice-title">{m.title}</span>
                <span class="provides">{m.help}</span>
              </span>
            </label>
          {/each}
        </div>
      {:else}
        <div class="problems"><PortProblems /></div>
        {#if ports === null}
          <p class="muted">{t('addPart.port.looking')}</p>
        {:else if ports.length === 0}
          <p class="muted">
            {t('addPart.port.none')}
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
                    >{#if p.kind === 'bluetooth'}<span class="kind"
                        >{t('addPart.port.bluetooth')}</span
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
      {/if}
    </fieldset>

    <fieldset class="step" class:flag={errorField === 'connector'}>
      <legend><span class="n">2</span> {t('addPart.connector.legend')}</legend>
      {#if core.connectorsError}
        <ErrorNotice error={core.connectorsError}>
          {#snippet actions()}
            <button type="button" class="btn" onclick={() => core.loadConnectors()}
              >{t('addPart.connector.retry')}</button
            >
          {/snippet}
        </ErrorNotice>
      {:else if connectors === null}
        <p class="muted">{t('addPart.connector.loading')}</p>
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
                  chooseConnector(c.id);
                  connectorTouched = true;
                  if (errorField === 'connector') error = null;
                }}
              />
              <span class="choice-body">
                <span class="choice-title">{c.name}</span>
                <span class="choice-meta">
                  <code>{c.id}</code>
                  <span>{t('addPart.connector.version', { version: c.version })}</span>
                  <span
                    >{c.port.kind === 'serial'
                      ? t('addPart.connector.serial', { baud: c.port.baud })
                      : t('addPart.connector.bridge')}</span
                  >
                </span>
                <span class="provides">
                  {t('addPart.connector.provides', {
                    capabilities: c.provides.map((p) => p.capability).join(', '),
                  })}
                </span>
              </span>
            </label>
          {/each}
        </div>
      {/if}
    </fieldset>

    {#if fields.length > 0}
      <fieldset class="step" class:flag={errorField === 'settings' || settingsProblem !== null}>
        <legend><span class="n">3</span> {t('addPart.settings.legend')}</legend>
        <p class="help settings-help">{t('addPart.settings.help')}</p>
        <div class="settings">
          {#each fields as f (f.key)}
            {@const id = `setting-${f.key}`}
            {@const bad = settingsProblem?.key === f.key}
            {#if f.kind === 'boolean'}
              <label class="check wide">
                <input
                  type="checkbox"
                  checked={values[f.key] === true}
                  onchange={(e) => (values = { ...values, [f.key]: e.currentTarget.checked })}
                />
                {f.label()}
              </label>
            {:else}
              <label class="label" for={id}>{f.label()}</label>
              <span class="value">
                {#if f.kind === 'number'}
                  <input
                    {id}
                    class="input num"
                    type="number"
                    step="any"
                    inputmode="decimal"
                    value={values[f.key]}
                    aria-invalid={bad}
                    oninput={(e) => {
                      values = { ...values, [f.key]: e.currentTarget.value };
                      if (bad) settingsProblem = null;
                    }}
                  />
                  <span class="unit">{f.unit}</span>
                {:else}
                  <select
                    {id}
                    class="input"
                    value={values[f.key]}
                    onchange={(e) => (values = { ...values, [f.key]: e.currentTarget.value })}
                  >
                    {#each f.choices as c (c.value)}
                      <option value={c.value}>{c.label()}</option>
                    {/each}
                  </select>
                {/if}
              </span>
              {#if bad}<p class="help bad wide">{settingsProblem?.message}</p>{/if}
            {/if}
          {/each}
        </div>
      </fieldset>
    {/if}

    <fieldset class="step" class:flag={errorField === 'name'}>
      <legend><span class="n">{fields.length > 0 ? 4 : 3}</span> {t('addPart.name.legend')}</legend>
      <div class="field">
        <label class="label" for="part-name">{t('addPart.name.label')}</label>
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
          placeholder={t('addPart.name.placeholder')}
        />
        <p id="part-name-help" class="help" class:bad={showNameProblem}>
          {#if showNameProblem}
            {nameProblem?.message} {nameProblem?.hint}
          {:else}
            {t('addPart.name.help')}
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
      {#if !port && !simulated}{t('addPart.footer.pickPort')}{:else if !connector}{t(
          'addPart.footer.pickConnector',
        )}{:else}<RichText text={t('addPart.footer.ready')} />{/if}
    </p>
    <button type="button" class="btn quiet" onclick={oncancel}>{t('addPart.footer.cancel')}</button>
    <button
      type="submit"
      class="btn primary"
      disabled={submitting || (!port && !simulated) || !connector}
    >
      {submitting ? t('addPart.footer.adding') : t('addPart.footer.submit')}
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
  .switch-row {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    margin-bottom: var(--space-3);
  }
  .switch-row label {
    cursor: pointer;
  }
  .switch-row.locked label {
    cursor: default;
  }
  .switch-row input {
    flex: none;
    width: 18px;
    height: 18px;
    margin: 3px 0 0;
    accent-color: var(--accent);
  }
  .modes {
    grid-template-columns: repeat(auto-fit, minmax(min(14rem, 100%), 1fr));
  }
  .settings-help {
    margin-bottom: var(--space-3);
  }
  .settings {
    display: grid;
    grid-template-columns: minmax(0, max-content) minmax(0, 1fr);
    align-items: center;
    gap: var(--space-2) var(--space-4);
  }
  .settings .wide {
    grid-column: 1 / -1;
  }
  .settings .label {
    font-size: var(--text-sm);
  }
  .value {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .input.num {
    width: 8rem;
    max-width: 100%;
  }
  .unit {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .check input {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
  @media (max-width: 480px) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
