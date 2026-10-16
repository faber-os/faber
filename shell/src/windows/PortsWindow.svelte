<script lang="ts">
  // Ports: every port of every kind, who claims it (on an I²C bus, who has which address), and
  // problems: things plugged in that never became a port, and ports something outside Faber has.
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { portAvailability, portKindLabel } from '../lib/state/portHints';
  import type { PortInfo } from '../lib/state/types';
  import { hexAddress } from '../lib/desktop/buses';
  import PortProblems from '../components/PortProblems.svelte';
  import type { WindowProps } from '../components/windowDefs';

  // Nothing to set up: it shows the machine's ports as they are.
  let props: WindowProps = $props();

  const core = useCore();
  const ports = $derived(core.ports);

  function who(p: PortInfo): string {
    if (p.reserved) return t('ports.reserved', { why: p.reserved });
    if (p.addresses && p.addresses.length > 0)
      return p.addresses
        .map((a) => t('ports.address', { address: hexAddress(a.address), part: a.claimedBy }))
        .join(', ');
    if (p.kind === 'i2c' && p.claimedBy == null) return t('ports.busFree');
    return portAvailability(p).text;
  }
</script>

<div class="ports">
  <PortProblems />
  {#if ports === null}
    <p class="pad">{t('parts.loading')}</p>
  {:else if ports.length === 0}
    <p class="pad">{t('ports.none')}</p>
  {:else}
    <table aria-label={t('ports.table.label')}>
      <thead>
        <tr>
          <th scope="col">{t('ports.col.port')}</th>
          <th scope="col">{t('ports.col.kind')}</th>
          <th scope="col">{t('ports.col.user')}</th>
        </tr>
      </thead>
      <tbody>
        {#each ports as p (p.id)}
          <tr data-port={p.id} class:problem={p.reserved != null}>
            <td>
              <span class="label">{p.label}</span>
              <code>{p.id}</code>
            </td>
            <td>{portKindLabel(p)}</td>
            <td>{who(p)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#if core.system?.buses === 'none'}
    <p class="note">{t('ports.noBuses')}</p>
  {:else if core.system?.buses === 'fake'}
    <p class="note">{t('ports.fakeBuses')}</p>
  {/if}
</div>

<style>
  .ports {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3);
    background: var(--surface);
  }
  .pad {
    padding: var(--space-3);
  }
  table {
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  th {
    text-align: left;
    color: var(--text-muted);
    font-weight: 600;
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }
  td code {
    display: block;
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .label {
    font-weight: 600;
  }
  tr.problem td {
    color: var(--text-muted);
  }
  .note {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
