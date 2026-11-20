<script lang="ts">
  let { lines }: { lines: string[] } = $props();
  let visible = $derived(lines.slice(-200));
</script>

<div class="job-log" role="log" aria-label="Job log" aria-live="off">
  {#each visible as line, i (`${i}-${line}`)}
    <div><span>{i + 1}</span><code>{line}</code></div>
  {/each}
  {#if visible.length === 0}<p class="muted">Waiting for log output…</p>{/if}
</div>

<style>
  .job-log { max-height: 260px; overflow: auto; padding: 8px; border: 1px solid var(--line); border-radius: 4px; background: var(--panel); font: 12px/1.5 ui-monospace, SFMono-Regular, Consolas, monospace; }
  .job-log > div { display: flex; gap: 10px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .job-log > div span { width: 2em; flex: none; color: var(--muted); text-align: right; }
  code { font: inherit; }
</style>