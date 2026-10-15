<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { baseInfo, motorInfo } from '../lib/state/partLimits';
  import {
    DriveLoop,
    LeaseKeeper,
    baseCommand,
    leaseEndText,
    motorCommand,
    type LeaseStatus,
  } from '../lib/state/driving';
  import type { ErrorBody, MotorReading, PlanarBaseReading } from '../lib/state/types';
  import {
    STILL,
    combine,
    driveKey,
    keysToStick,
    scaleStick,
    type DriveKey,
    type Stick,
  } from '../lib/desktop/driveInput';
  import { formatTurnRate } from '../lib/viewers/units';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import JoystickPad from '../components/JoystickPad.svelte';
  import RichText from '../components/RichText.svelte';
  import TwistGauges from '../components/TwistGauges.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const READINGS_HZ = 10;

  const core = useCore();
  const desktop = useDesktop();

  const movable = $derived(core.movableParts);
  const part = $derived(movable.find((p) => p.name === win.params.part) ?? movable[0] ?? null);
  const partName = $derived(part?.name ?? null);
  const isBase = $derived(part?.signals.some((s) => s.capability === 'planar-base@1') ?? false);
  const base = $derived(part && isBase ? baseInfo(part) : null);
  const motor = $derived(part && !isBase ? motorInfo(part) : null);
  const holonomic = $derived(base?.holonomic ?? false);
  const signal = $derived(
    part?.signals.find((s) => s.capability === 'planar-base@1' || s.capability === 'motor@1') ??
      null,
  );
  const machine = $derived(core.settings?.machine.name ?? 'faber');
  const unit = $derived(core.settings?.units.length ?? 'metric');
  const estopped = $derived(core.estop?.engaged === true);
  /** False while the browser itself is in the background (another app or tab in front). */
  let pageFocused = $state(true);
  /** Driving only happens in the window in front, while it shows, once D46 is answered. */
  const active = $derived(
    core.drivingConfirmed &&
      visible &&
      pageFocused &&
      desktop.state.focusedId === win.id &&
      !estopped,
  );

  let status = $state<LeaseStatus>({ kind: 'free' });
  let asking = $state(false);
  let declined = $state(false);
  let speed = $state(50);
  let pad = $state<Stick>(STILL);
  let turn = $state(0);
  let held = $state.raw<ReadonlySet<DriveKey>>(new Set());
  let baseReading = $state<PlanarBaseReading | null>(null);
  let motorReading = $state<MotorReading | null>(null);
  let readingError = $state<ErrorBody | null>(null);

  let keeper: LeaseKeeper | null = null;
  let loop: DriveLoop | null = null;

  // D46: ask once per session, the first time a part could be driven here.
  $effect(() => {
    if (part && !core.drivingConfirmed && !declined) asking = true;
  });

  // One lease keeper per part; closing the window or picking another part lets go.
  $effect(() => {
    const name = partName;
    if (!name) return;
    const k = untrack(() => new LeaseKeeper(core, name, t('drive.holder'), (s) => (status = s)));
    const l = new DriveLoop(k);
    keeper = k;
    loop = l;
    status = k.status;
    const stopWatching = core.watchLeaseEnds((lease, reason) => k.onLeaseEnded(lease, reason));
    return () => {
      stopWatching();
      void l.release();
      if (keeper === k) keeper = loop = null;
    };
  });

  $effect(() => {
    if (!signal) return;
    baseReading = null;
    motorReading = null;
    return core.watchSignal(signal.address, READINGS_HZ, {
      onReading: (r) => {
        readingError = null;
        if (r.capability === 'planar-base@1') baseReading = r.reading;
        else if (r.capability === 'motor@1') motorReading = r.reading;
      },
      onError: (e) => (readingError = e),
    });
  });

  const stick = $derived(combine(pad, keysToStick(held), { x: 0, y: 0, turn }));
  const command = $derived(
    base
      ? baseCommand(scaleStick(stick, base.maxVelocity, speed / 100, holonomic))
      : motorCommand(
          Math.round(stick.x * (motor?.maxVelocityRadS ?? 0) * (speed / 100) * 1000) / 1000,
        ),
  );

  // Send what the controls ask for; nothing moves unless this window may drive.
  $effect(() => {
    const c = command;
    if (!active) return;
    untrack(() => loop?.set(c));
  });

  // Losing focus, hiding the window or the e-stop: stop and let go of the lease.
  $effect(() => {
    if (active) return;
    untrack(() => {
      clearControls();
      void loop?.release();
    });
  });

  // Once the e-stop is released, why driving stopped before it is old news: ready again.
  let wasStopped = false;
  $effect(() => {
    const now = estopped;
    if (wasStopped && !now) untrack(() => keeper?.clear());
    wasStopped = now;
  });

  function clearControls() {
    pad = STILL;
    turn = 0;
    held = new Set();
  }

  function start() {
    core.confirmDriving();
    asking = false;
    declined = false;
  }

  function cancel() {
    asking = false;
    declined = true;
  }

  function takeOver() {
    keeper?.clear();
    void keeper?.acquire(true);
  }

  function stopEverything() {
    void core.engageEstop(t('estop.reason.drive'));
  }

  /** Keys that belong to the element itself: typing, and the speed slider's arrows. */
  function ownsKeys(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    if (target.isContentEditable || target.closest('[data-own-keys]')) return true;
    return (
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement ||
      (target instanceof HTMLInputElement && !['range', 'checkbox', 'radio'].includes(target.type))
    );
  }

  function keydown(e: KeyboardEvent) {
    if (!active || asking || ownsKeys(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;
    const k = driveKey(e);
    if (!k) return;
    e.preventDefault();
    if (k === 'stop') {
      clearControls();
      return;
    }
    if (!held.has(k)) held = new Set([...held, k]);
  }

  function keyup(e: KeyboardEvent) {
    const k = driveKey(e);
    if (!k || !held.has(k)) return;
    const next = new Set(held);
    next.delete(k);
    held = next;
  }

  const other = $derived(partName ? core.leaseOn(partName) : null);
  const statusLine = $derived.by((): { text: string; tone: 'ok' | 'warn' | 'bad' | 'quiet' } => {
    if (!partName) return { text: '', tone: 'quiet' };
    if (estopped) return { text: t('drive.ended.estop'), tone: 'bad' };
    if (status.kind === 'held')
      return { text: t('drive.status.driving', { part: partName }), tone: 'ok' };
    if (status.kind === 'requesting')
      return { text: t('drive.status.requesting', { part: partName }), tone: 'quiet' };
    if (other) return { text: t('drive.status.drivenBy', { holder: other.holder }), tone: 'warn' };
    if (status.kind === 'ended')
      return { text: leaseEndText(status.reason, partName, status.error), tone: 'warn' };
    if (!core.drivingConfirmed) return { text: t('drive.notStarted'), tone: 'quiet' };
    return { text: t('drive.status.ready', { part: partName }), tone: 'quiet' };
  });
  const refused = $derived(
    status.kind === 'refused' && status.error.code !== 'lease-held' && !estopped
      ? status.error
      : null,
  );
  const keysHelp = $derived(
    motor ? t('drive.keys.motor') : holonomic ? t('drive.keys') : t('drive.keys.noSlide'),
  );
</script>

<svelte:window
  onkeydown={keydown}
  onkeyup={keyup}
  onblur={() => (pageFocused = false)}
  onfocus={() => (pageFocused = true)}
/>

<div class="drive">
  {#if !part}
    <div class="empty">
      <h3>{t('drive.pick.none.title')}</h3>
      <p>{t('drive.pick.none.body')}</p>
      <button class="btn primary" onclick={() => desktop.open('parts')}>
        {t('drive.pick.openParts')}
      </button>
    </div>
  {:else}
    <div class="top">
      <label class="pick">
        <span class="label">{t('drive.pick.label')}</span>
        <select
          class="input"
          value={partName}
          data-own-keys
          onchange={(e) => desktop.update(win.id, { params: { part: e.currentTarget.value } })}
        >
          {#each movable as p (p.name)}
            <option value={p.name}>{p.name}</option>
          {/each}
        </select>
      </label>
      <button
        class="btn danger solid stop"
        title={t('drive.stop.title')}
        onclick={stopEverything}
        disabled={estopped}
      >
        <Icon name="stop" size={18} />
        {t('drive.stop')}
      </button>
    </div>

    <div class="status {statusLine.tone}" role="status" data-testid="drive-status">
      <span class="dot" aria-hidden="true"></span>
      <span class="text">{statusLine.text}</span>
      {#if other && status.kind !== 'held' && !estopped}
        <button class="btn" onclick={takeOver}>{t('drive.takeOver')}</button>
      {:else if !core.drivingConfirmed}
        <button class="btn primary" onclick={() => (asking = true)}>{t('drive.start')}</button>
      {/if}
    </div>
    {#if estopped}<p class="hint">{t('drive.status.estopHint')}</p>{/if}
    {#if refused}<div class="pad-x"><ErrorNotice error={refused} /></div>{/if}

    <div class="controls">
      <div class="stickbox">
        <JoystickPad {holonomic} disabled={!active} onchange={(s) => (pad = s)} />
        {#if !motor}
          <label class="slider turn">
            <span class="label">{t('drive.turn.label')}</span>
            <input
              type="range"
              min="-100"
              max="100"
              step="1"
              value={Math.round(-turn * 100)}
              disabled={!active}
              oninput={(e) => (turn = -Number(e.currentTarget.value) / 100)}
              onpointerup={() => (turn = 0)}
              onpointercancel={() => (turn = 0)}
              onblur={() => (turn = 0)}
            />
          </label>
        {/if}
      </div>

      <div class="side">
        <label class="slider" data-own-keys>
          <span class="label">
            {t('drive.speed.label')}
            <span class="num">{t('drive.speed.value', { percent: speed })}</span>
          </span>
          <input type="range" min="10" max="100" step="10" bind:value={speed} />
        </label>

        {#if readingError}
          <ErrorNotice error={readingError} />
        {:else if base}
          <TwistGauges
            commanded={baseReading?.commanded ?? null}
            measured={baseReading?.measured ?? null}
            max={base.maxVelocity}
            {unit}
            {holonomic}
            compact
          />
          {#if baseReading?.bumped}
            <p class="bumped"><Icon name="warning" size={16} /> {t('twist.bumped')}</p>
          {/if}
        {:else if motor}
          <p class="motor">
            <span class="label">{t('twist.motor')}</span>
            <span class="num" data-testid="motor-velocity"
              >{motorReading?.velocityRadS === undefined
                ? '–'
                : formatTurnRate(motorReading.velocityRadS)}</span
            >
          </p>
        {/if}
      </div>
    </div>

    <p class="keys"><RichText text={keysHelp} /></p>
  {/if}
</div>

{#if asking && part}
  <Dialog title={t('drive.confirm.title', { machine })} oncancel={cancel}>
    <p>{t('drive.confirm.body', { part: part.name })}</p>
    {#snippet actions()}
      <button class="btn" onclick={cancel}>{t('drive.confirm.cancel')}</button>
      <button class="btn primary" onclick={start}>{t('drive.confirm.start')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .drive {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4) var(--space-4);
    background: var(--surface);
  }
  .empty {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-2);
  }
  .empty h3 {
    font-size: var(--text-xl);
    font-weight: 680;
  }
  .empty p {
    color: var(--text-muted);
    max-width: 48ch;
  }
  .top {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-3);
  }
  .pick {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .label {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .stop {
    min-height: 52px;
    padding: 0 var(--space-5);
    font-size: var(--text-lg);
    font-weight: 750;
  }
  .status {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    min-height: 40px;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--surface-2);
    font-weight: 620;
  }
  .status .text {
    flex: 1;
    min-width: 12rem;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .ok .dot {
    background: var(--ok);
  }
  .warn .dot {
    background: var(--warn);
  }
  .bad {
    color: var(--danger);
  }
  .bad .dot {
    background: var(--danger);
  }
  .hint {
    margin-top: calc(-1 * var(--space-2));
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .pad-x {
    display: grid;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    align-items: flex-start;
  }
  .stickbox {
    display: grid;
    justify-items: center;
    gap: var(--space-2);
    flex: 1 1 180px;
    max-width: 240px;
    margin: 0 auto;
  }
  .side {
    display: grid;
    gap: var(--space-3);
    flex: 1 1 220px;
    min-width: 0;
  }
  .slider {
    display: grid;
    gap: 2px;
    width: 100%;
  }
  .slider input {
    width: 100%;
    accent-color: var(--accent);
  }
  .num {
    font-variant-numeric: tabular-nums;
    font-weight: 650;
    color: var(--text);
  }
  .motor {
    display: flex;
    justify-content: space-between;
  }
  .bumped {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--warn);
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .keys {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
