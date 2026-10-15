<script lang="ts">
  let { size = 18 }: { size?: number } = $props();
</script>

<!-- No logo exists yet: a lowercase wordmark with one ember spark, like a struck hot iron. -->
<span class="wordmark" style:font-size="{size}px" role="img" aria-label="Faber">
  <span aria-hidden="true">faber</span><span class="spark" aria-hidden="true"></span>
</span>

<style>
  .wordmark {
    display: inline-flex;
    align-items: baseline;
    font-weight: 760;
    letter-spacing: -0.045em;
    line-height: 1;
    color: var(--text);
  }
  .spark {
    width: 0.26em;
    height: 0.26em;
    margin-left: 0.08em;
    border-radius: 0.06em;
    background: var(--accent);
    transform: rotate(45deg) translateY(-0.02em);
  }
</style>
