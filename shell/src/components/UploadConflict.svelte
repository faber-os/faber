<script lang="ts">
  import { t } from '../lib/i18n';
  import type { ConflictAnswer } from '../lib/desktop/uploads';
  import Dialog from './Dialog.svelte';

  /** "x is already in folder": Replace, Keep both or Skip, for Files and desktop drops. */
  let {
    name,
    folder,
    remaining,
    answer,
  }: {
    name: string;
    folder: string;
    /** Files still to come: then "do the same for the others" is offered. */
    remaining: number;
    answer: (a: ConflictAnswer) => void;
  } = $props();

  let forAll = $state(false);
</script>

<Dialog
  title={t('files.dialog.conflict.title', { name, folder })}
  oncancel={() => answer({ choice: 'skip', forAll: false })}
>
  <p>{t('files.dialog.conflict.body')}</p>
  {#if remaining > 0}
    <label class="ack">
      <input type="checkbox" class="check" bind:checked={forAll} />
      {t('files.dialog.conflict.forAll')}
    </label>
  {/if}
  {#snippet actions()}
    <button class="btn" onclick={() => answer({ choice: 'skip', forAll })}
      >{t('files.dialog.conflict.skip')}</button
    >
    <button class="btn danger" onclick={() => answer({ choice: 'replace', forAll })}
      >{t('files.dialog.conflict.replace')}</button
    >
    <button class="btn primary" onclick={() => answer({ choice: 'keep-both', forAll })}
      >{t('files.dialog.conflict.keepBoth')}</button
    >
  {/snippet}
</Dialog>

<style>
  .ack {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin-top: var(--space-3);
    font-weight: 600;
  }
</style>
