<script lang="ts">
  import { richParts } from '../lib/i18n';

  /** A catalogue text that may hold `<b>…</b>` and `<code>…</code>`; never parsed as HTML. */
  let { text }: { text: string } = $props();
</script>

{#each richParts(text) as part, i (i)}{#if part.tag === 'b'}<strong>{part.text}</strong
    >{:else if part.tag === 'code'}<code>{part.text}</code>{:else}{part.text}{/if}{/each}
