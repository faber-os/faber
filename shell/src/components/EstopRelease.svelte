<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import Dialog from './Dialog.svelte';

  /** "Release the e-stop?": an operator's decision (docs/auth.md), so the shell asks first. */
  let { onclose }: { onclose: () => void } = $props();

  const core = useCore();

  async function release() {
    onclose();
    const r = await core.releaseEstop();
    if (!r.ok) core.lastError = r.error;
  }
</script>

<!-- Beside the taskbar, not in it, so the question covers the whole desktop above every window. -->
<div class="sheet">
  <Dialog title={t('estop.release.title')} oncancel={onclose} tone="danger">
    <p>{t('estop.release.body')}</p>
    {#snippet actions()}
      <button class="btn" data-autofocus onclick={onclose}>{t('estop.release.cancel')}</button>
      <button class="btn danger solid" onclick={release}>{t('estop.release.confirm')}</button>
    {/snippet}
  </Dialog>
</div>

<style>
  .sheet {
    position: absolute;
    inset: 0;
    z-index: 100000;
  }
</style>
