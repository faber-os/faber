<script lang="ts">
  // The I2C scanner: an i2cdetect-style grid of one bus, what each answer might be, and Add part.
  // Only ever scans when the user presses Scan (docs/api.md, "I²C scan").
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, I2cScan } from '../lib/state/types';
  import { hexAddress, scanGrid } from '../lib/desktop/buses';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import RichText from '../components/RichText.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();

  const buses = $derived((core.ports ?? []).filter((p) => p.kind === 'i2c'));
  let chosen = $state<string | null>(null);
  const bus = $derived(buses.find((b) => b.id === (chosen ?? win.params.port)) ?? buses[0] ?? null);
  let scanning = $state(false);
  let scan = $state<I2cScan | null>(null);
  let error = $state<ErrorBody | null>(null);
  const grid = $derived(scanGrid(scan && scan.port === bus?.id ? scan : null));
  const found = $derived(scan && scan.port === bus?.id ? scan.found : []);

  async function run() {
    if (!bus) return;
    scanning = true;
    error = null;
    const r = await core.scanI2c(bus.id);
    scanning = false;
    if (r.ok) scan = r.value;
    else error = r.error;
  }

  function addPart(connector: string, address: number) {
    if (!bus) return;
    const id = desktop.open('parts');
    desktop.update(id, {
      params: { addPort: bus.id, addConnector: connector, addAddress: String(address) },
    });
  }

  function connectorName(id: string): string {
    return core.connectors?.find((c) => c.id === id)?.name ?? id;
  }

  const COLUMNS = Array.from({ length: 16 }, (_, i) => i.toString(16));
</script>

<div class="scanner">
  {#if buses.length === 0}
    <div class="pad empty" role="status">
      <h3>{t('i2cScanner.noBus.title')}</h3>
      <p>
        {core.system?.buses === 'none'
          ? t('i2cScanner.noBus.none')
          : core.system?.buses === 'fake'
            ? t('i2cScanner.noBus.fake')
            : t('i2cScanner.noBus.system')}
      </p>
    </div>
  {:else}
    <div class="toolbar">
      {#if buses.length > 1}
        <label>
          {t('i2cScanner.bus')}
          <select
            value={bus?.id}
            onchange={(e) => (chosen = (e.currentTarget as HTMLSelectElement).value)}
          >
            {#each buses as b (b.id)}
              <option value={b.id}>{b.label}</option>
            {/each}
          </select>
        </label>
      {:else}
        <span class="bus">{bus?.label}</span>
      {/if}
      <button class="btn primary" onclick={run} disabled={scanning}>
        <Icon name="search" size={16} />
        {scanning ? t('i2cScanner.scanning') : t('i2cScanner.scan')}
      </button>
    </div>
    <p class="note">{t('i2cScanner.safe')}</p>
    {#if error}<div class="pad"><ErrorNotice {error} /></div>{/if}

    <table class="grid" aria-label={t('i2cScanner.grid.label')}>
      <thead>
        <tr>
          <th></th>
          {#each COLUMNS as c (c)}<th scope="col">{c}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each grid as row, r (r)}
          <tr>
            <th scope="row">{(r * 16).toString(16).padStart(2, '0')}</th>
            {#each row as cell (cell.address)}
              <td
                class={cell.state}
                title={cell.state === 'used'
                  ? t('i2cScanner.cell.used', {
                      address: hexAddress(cell.address),
                      part: cell.usedBy ?? '',
                    })
                  : cell.state === 'found'
                    ? t('i2cScanner.cell.found', { address: hexAddress(cell.address) })
                    : undefined}
                data-testid={cell.state === 'found' || cell.state === 'used'
                  ? `i2c-${hexAddress(cell.address)}`
                  : undefined}
              >
                {#if cell.state === 'found'}{cell.address.toString(16).padStart(2, '0')}
                {:else if cell.state === 'used'}{t('i2cScanner.cell.usedMark')}
                {:else if cell.state === 'empty' && scan}{t('i2cScanner.cell.none')}{/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>

    {#if scan && scan.port === bus?.id}
      <section class="found" aria-label={t('i2cScanner.found.label')}>
        {#if found.length === 0}
          <p class="pad">{t('i2cScanner.found.nothing')}</p>
        {:else}
          <p class="hint"><RichText text={t('i2cScanner.found.guess')} /></p>
          <ul>
            {#each found as f (f.address)}
              <li data-testid="i2c-found-{hexAddress(f.address)}">
                <strong>{hexAddress(f.address)}</strong>
                {#if f.usedBy}
                  <span>{t('i2cScanner.found.usedBy', { part: f.usedBy })}</span>
                {:else if f.suggestions.length === 0}
                  <span class="muted">{t('i2cScanner.found.unknown')}</span>
                {:else}
                  <ul class="guesses">
                    {#each f.suggestions as s, i (i)}
                      <li>
                        <span>{t('i2cScanner.found.maybe', { chip: s.chip })}</span>
                        <span class="muted reason">{s.reason}</span>
                        {#if s.connector}
                          <button
                            class="btn"
                            onclick={() => addPart(s.connector!, f.address)}
                            aria-label={t('i2cScanner.addPart.label', {
                              connector: connectorName(s.connector!),
                              address: hexAddress(f.address),
                            })}
                          >
                            <Icon name="plus" size={15} />
                            {t('i2cScanner.addPart')}
                          </button>
                        {:else}
                          <span class="muted">{t('i2cScanner.found.noConnector')}</span>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  .scanner {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--surface);
  }
  .pad {
    padding: var(--space-4);
  }
  .empty {
    display: grid;
    gap: var(--space-2);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .bus {
    flex: 1;
    font-weight: 600;
  }
  label {
    flex: 1;
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .note,
  .hint {
    margin: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .grid {
    margin: var(--space-2) var(--space-4);
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
  .grid th {
    color: var(--text-muted);
    font-weight: 400;
    padding: 2px 6px;
  }
  .grid td {
    text-align: center;
    width: 2.2em;
    height: 1.8em;
    border: 1px solid var(--line);
    color: var(--text-faint);
  }
  .grid td.skipped {
    background: var(--surface-2);
  }
  .grid td.found {
    background: var(--accent);
    color: var(--on-accent, white);
    font-weight: 700;
  }
  .grid td.used {
    background: var(--surface-3);
    color: var(--text);
    font-weight: 700;
  }
  .found ul {
    margin: 0;
    padding: 0 var(--space-4) var(--space-4);
    list-style: none;
    display: grid;
    gap: var(--space-2);
  }
  .found ul.guesses {
    padding: var(--space-1) 0 0 var(--space-4);
  }
  .guesses li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .muted {
    color: var(--text-muted);
  }
  .reason {
    flex-basis: 100%;
    font-size: var(--text-xs);
  }
</style>
