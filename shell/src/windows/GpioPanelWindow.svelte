<script lang="ts">
  // The GPIO panel: the Pi's 40-pin header with pin numbers and names. Inputs show their level
  // live; an output can be switched only through its part, with a lease, after the D46 confirm.
  // The lease lasts while this window holds the output on; when it ends, the output goes back
  // to its safe level (inactive) by itself.
  import { onDestroy } from 'svelte';
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, HeaderPin, PartInfo } from '../lib/state/types';
  import { gpioPort, headerRows, pinKind } from '../lib/desktop/buses';
  import { useDesktop } from '../components/desktopController.svelte';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { visible }: WindowProps = $props();

  const HOLDER = 'gpio-panel';
  /** How often a held output's lease is renewed (the lease is 1 s). */
  const RENEW_MS = 300;

  const core = useCore();
  const useLock = $derived(core.auth.lock('operator'));
  const addLock = $derived(core.auth.lock('admin'));
  const desktop = useDesktop();

  let pins = $state<HeaderPin[] | null>(null);
  let error = $state<ErrorBody | null>(null);
  $effect(() => {
    void core.loadHeader().then((r) => {
      if (r.ok) pins = r.value;
      else error = r.error;
    });
  });
  const rows = $derived(headerRows(pins ?? []));

  /** The part on each GPIO port. */
  const partOn = $derived(
    new Map((core.parts ?? []).filter((p) => p.port?.startsWith('gpio-')).map((p) => [p.port!, p])),
  );
  function signalOf(p: PartInfo): string | null {
    return (
      p.signals.find((s) => s.capability === 'digital-in@1' || s.capability === 'digital-out@1')
        ?.address ?? null
    );
  }

  /** Live levels, by signal address. */
  let levels = $state<Record<string, boolean>>({});
  const addresses = $derived(
    [...partOn.values()].map(signalOf).filter((a): a is string => a !== null),
  );
  $effect(() => {
    const stops = addresses.map((address) =>
      core.watchSignal(address, 20, {
        onReading: (r) => {
          if (r.capability === 'digital-in@1' || r.capability === 'digital-out@1')
            levels = { ...levels, [address]: r.reading.level };
        },
        onError: () => {},
      }),
    );
    return () => stops.forEach((s) => s());
  });

  /** The output this window holds on, with its lease. */
  let held = $state<{ part: string; lease: string } | null>(null);
  let timer: ReturnType<typeof setInterval> | null = null;
  let asking = $state<{ part: string; level: boolean } | null>(null);
  let commandError = $state<ErrorBody | null>(null);

  function setCommand(level: boolean) {
    return { capability: 'digital-out@1' as const, command: { kind: 'set' as const, level } };
  }

  async function release() {
    if (timer) clearInterval(timer);
    timer = null;
    const h = held;
    held = null;
    if (h) await core.releaseLease(h.part, h.lease);
  }

  function toggle(part: string, level: boolean) {
    if (!core.drivingConfirmed) {
      asking = { part, level };
      return;
    }
    void apply(part, level);
  }

  async function apply(part: string, level: boolean) {
    commandError = null;
    if (!level) return release();
    if (held && held.part !== part) await release();
    if (!held) {
      const r = await core.requestLease(part, HOLDER);
      if (!r.ok) return void (commandError = r.error);
      held = { part, lease: r.value.lease };
    }
    const h = held;
    const r = await core.command(h.part, h.lease, setCommand(true));
    if (!r.ok) {
      commandError = r.error;
      return release();
    }
    // Every command renews the lease: keep sending "on" while the window holds it.
    if (timer) clearInterval(timer);
    timer = setInterval(async () => {
      if (!held || !visible) return void release();
      const again = await core.command(held.part, held.lease, setCommand(true));
      if (!again.ok) {
        commandError = again.error;
        void release();
      }
    }, RENEW_MS);
  }

  function confirm() {
    core.confirmDriving();
    const a = asking;
    asking = null;
    if (a) void apply(a.part, a.level);
  }

  onDestroy(() => void release());

  function addPart(line: number) {
    const id = desktop.open('parts');
    desktop.update(id, { params: { addPort: gpioPort(line) } });
  }
</script>

<div class="panel">
  {#if core.system?.buses === 'none'}
    <p class="note" role="status">{t('gpio.noBuses')}</p>
  {:else if core.system?.buses === 'fake'}
    <p class="note">{t('gpio.fake')}</p>
  {/if}
  {#if error}<ErrorNotice {error} />{/if}
  {#if commandError}<ErrorNotice error={commandError} />{/if}
  <p class="note">{t('gpio.help')}</p>
  <table class="header" aria-label={t('gpio.header.label')}>
    <tbody>
      {#each rows as row (row[0].pin)}
        <tr>
          {#each row as pin, side (pin.pin)}
            {@const line = pin.line ?? null}
            {@const port =
              line === null ? null : (core.ports ?? []).find((p) => p.id === gpioPort(line))}
            {@const part = line === null ? null : (partOn.get(gpioPort(line)) ?? null)}
            {@const address = part ? signalOf(part) : null}
            {@const level = address ? levels[address] : undefined}
            {@const isOutput = part?.signals.some((s) => s.capability === 'digital-out@1') ?? false}
            <td class="cell {pinKind(pin)} {side === 0 ? 'left' : 'right'}" data-pin={pin.pin}>
              <span class="num">{pin.pin}</span>
              <span class="name">{pin.name}</span>
              {#if pin.function}<span class="fn">{pin.function}</span>{/if}
              {#if line !== null}
                {#if part}
                  <span class="part">{part.name}</span>
                  {#if level !== undefined}
                    <span
                      class="lamp"
                      class:on={level}
                      role="img"
                      aria-label={level ? t('level.on') : t('level.off')}
                    ></span>
                  {/if}
                  {#if isOutput}
                    <button
                      class="btn"
                      aria-pressed={held?.part === part.name}
                      disabled={useLock !== null}
                      title={useLock ?? undefined}
                      onclick={() => toggle(part.name, held?.part !== part.name)}
                    >
                      {held?.part === part.name ? t('gpio.output.off') : t('gpio.output.on')}
                    </button>
                  {/if}
                {:else if port?.reserved}
                  <span class="muted">{port.reserved}</span>
                {:else if port}
                  <button
                    class="btn quiet"
                    disabled={addLock !== null}
                    title={addLock ?? undefined}
                    onclick={() => addPart(line)}>{t('gpio.addPart')}</button
                  >
                {/if}
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>

{#if asking}
  <Dialog title={t('gpio.confirm.title')} oncancel={() => (asking = null)}>
    <p>{t('gpio.confirm.body', { part: asking.part })}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (asking = null)}>{t('drive.confirm.cancel')}</button>
      <button class="btn primary" onclick={confirm}>{t('gpio.confirm.start')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3);
    background: var(--surface);
  }
  .note {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .header {
    border-collapse: separate;
    border-spacing: 2px;
    font-size: var(--text-sm);
  }
  .cell {
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    white-space: nowrap;
  }
  .cell > * {
    margin-right: var(--space-1);
  }
  .right {
    text-align: left;
  }
  .num {
    display: inline-block;
    min-width: 1.6em;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .name {
    font-weight: 600;
  }
  .power .name {
    color: var(--warn);
  }
  .ground .name {
    color: var(--text-muted);
  }
  .fn,
  .muted {
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .part {
    font-weight: 600;
    color: var(--accent);
  }
  .lamp {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    vertical-align: middle;
    background: var(--surface-3);
    border: 1px solid var(--line);
  }
  .lamp.on {
    background: var(--accent);
    border-color: var(--accent);
  }
</style>
