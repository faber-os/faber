<script lang="ts">
  import { t } from '../lib/i18n';
  import Icon from './Icon.svelte';

  /** The × that closes a notice about a lasting problem (see lib/desktop/dismissals). */
  let { ondismiss }: { ondismiss: () => void } = $props();
</script>

<button
  type="button"
  class="dismiss btn quiet icon"
  aria-label={t('notice.dismiss')}
  title={t('notice.dismiss.tooltip')}
  onclick={ondismiss}
>
  <Icon name="close" size={16} />
</button>

<style>
  .dismiss {
    flex: none;
    width: 32px;
    min-height: 32px;
    margin: calc(-1 * var(--space-1)) 0;
    color: var(--text-muted);
  }
</style>
