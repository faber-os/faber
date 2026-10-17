<script lang="ts">
  /** The parts list: click to add a part in the middle of the view, or drag it onto the mat. */
  import { t } from '../../lib/i18n';
  import { useCircuit } from './circuit.svelte';
  import { G, art } from './looks';

  const lab = useCircuit();

  let q = $state('');
  const list = $derived(
    lab.allDefs.filter(
      (d) => !q.trim() || `${d.name} ${d.covers}`.toLowerCase().includes(q.trim().toLowerCase()),
    ),
  );
</script>

<div class="head">
  <h2>{t('circuit.library.title')}</h2>
  <input
    type="search"
    placeholder={t('circuit.library.search')}
    aria-label={t('circuit.library.search')}
    bind:value={q}
  />
</div>
<div class="list">
  {#each list as d (d.id)}
    <button
      class="item"
      title={t('circuit.library.add', { name: d.name })}
      draggable="true"
      ondragstart={(e) => e.dataTransfer?.setData('text/x-faber-part', d.id)}
      onclick={() => lab.addPart(d.id)}
    >
      <svg viewBox="-6 -6 {d.w * G + 12} {d.h * G + 12}" aria-hidden="true"
        >{@html art(d).replaceAll('url(#mesh)', '#5b6068')}</svg
      >
      <span><b>{d.name}</b><span>{d.covers}</span></span>
    </button>
  {:else}
    <p class="none">{t('circuit.library.none')}</p>
  {/each}
</div>
<div class="foot">
  <button class="btn" onclick={() => (lab.newPartOpen = true)}
    >{t('circuit.library.newPart')}</button
  >
</div>

<style>
  .head {
    padding: 12px 12px 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    font-size: 11.5px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-muted);
    margin: 0;
  }
  input {
    width: 100%;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-md);
    padding: 6px 9px;
    background: var(--surface-2);
    font-size: 13px;
    color: var(--text);
  }
  .list {
    overflow: auto;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    flex: 1;
  }
  .item {
    display: grid;
    grid-template-columns: 48px minmax(0, 1fr);
    gap: 10px;
    align-items: center;
    text-align: left;
    border: 0;
    background: none;
    border-radius: var(--radius-md);
    padding: 6px;
    cursor: grab;
    color: inherit;
  }
  .item:hover {
    background: var(--surface-3);
  }
  svg {
    width: 48px;
    height: 36px;
    display: block;
  }
  b {
    display: block;
    font-size: 13px;
    font-weight: 600;
  }
  .item span span {
    display: block;
    font-size: 11.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    padding: 8px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .foot {
    padding: 10px 12px;
    border-top: 1px solid var(--line);
  }
  .foot .btn {
    width: 100%;
    justify-content: center;
  }
</style>
