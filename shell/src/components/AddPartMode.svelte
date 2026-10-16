<script lang="ts">
  import { t } from '../lib/i18n';

  /**
   * Add part's two ways (docs/wiring.md §2): a part on a port (a connector), or a part on a
   * board's pins (guided mode). Shown in both forms' toolbars.
   */
  let { mode, onchange }: { mode: 'port' | 'pins'; onchange: (mode: 'port' | 'pins') => void } =
    $props();
</script>

<div class="segmented" role="radiogroup" aria-label={t('addPart.mode.label')}>
  <label
    ><input
      type="radio"
      name="add-part-mode"
      checked={mode === 'port'}
      onchange={() => onchange('port')}
    />{t('addPart.mode.port')}</label
  >
  <label
    ><input
      type="radio"
      name="add-part-mode"
      checked={mode === 'pins'}
      onchange={() => onchange('pins')}
    />{t('addPart.mode.pins')}</label
  >
</div>
