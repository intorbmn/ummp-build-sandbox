<script lang="ts">
  type DiffLine = { number: number; text: string };
  type Segment =
    | { kind: 'line'; line: DiffLine }
    | { kind: 'collapsed'; lines: DiffLine[] };

  let { diff }: { diff: string } = $props();
  let lines = $derived(diff.split('\n')
    .filter((line, i, all) => !(line === '' && i === all.length - 1))
    .map((text, index) => ({ number: index + 1, text })));

  function buildSegments(rows: DiffLine[]): Segment[] {
    const result: Segment[] = [];
    let index = 0;
    while (index < rows.length) {
      if (!rows[index].text.startsWith(' ')) {
        result.push({ kind: 'line', line: rows[index] });
        index += 1;
        continue;
      }
      let end = index;
      while (end < rows.length && rows[end].text.startsWith(' ')) end += 1;
      const run = rows.slice(index, end);
      if (run.length > 10) {
        result.push(...run.slice(0, 3).map((line) => ({ kind: 'line' as const, line })));
        result.push({ kind: 'collapsed', lines: run.slice(3, -3) });
        result.push(...run.slice(-3).map((line) => ({ kind: 'line' as const, line })));
      } else {
        result.push(...run.map((line) => ({ kind: 'line' as const, line })));
      }
      index = end;
    }
    return result;
  }

  let segments = $derived(buildSegments(lines));
</script>

<div class="diff" role="region" aria-label="File diff">
  {#each segments as segment, i (i)}
    {#if segment.kind === 'line'}
      <div class:added={segment.line.text.startsWith('+') && !segment.line.text.startsWith('+++')}
        class:removed={segment.line.text.startsWith('-') && !segment.line.text.startsWith('---')}
        class:hunk={segment.line.text.startsWith('@@')}
        class:meta={segment.line.text.startsWith('---') || segment.line.text.startsWith('+++')}>
        <span class="number" aria-hidden="true">{segment.line.number}</span>
        <span class="marker" aria-hidden="true">{segment.line.text.startsWith('+') && !segment.line.text.startsWith('+++') ? '+' : segment.line.text.startsWith('-') && !segment.line.text.startsWith('---') ? '−' : ' '}</span>
        <code>{segment.line.text}</code>
      </div>
    {:else}
      <details class="collapsed">
        <summary>{segment.lines.length} unchanged lines hidden</summary>
        {#each segment.lines as line (line.number)}
          <div>
            <span class="number" aria-hidden="true">{line.number}</span>
            <span class="marker" aria-hidden="true"> </span>
            <code>{line.text}</code>
          </div>
        {/each}
      </details>
    {/if}
  {/each}
  {#if lines.length === 0}<p class="muted">No textual changes.</p>{/if}
</div>

<style>
  .diff { overflow: auto; max-height: 56vh; border: 1px solid var(--line); border-radius: 5px; background: var(--panel); font: 12px/1.55 ui-monospace, SFMono-Regular, Consolas, monospace; }
  .diff > div, .collapsed > div { display: grid; grid-template-columns: 3.2em 1.5em minmax(max-content, 1fr); white-space: pre; padding-right: 10px; }
  .number { text-align: right; padding-right: 8px; color: var(--muted); user-select: none; border-right: 1px solid var(--line); }
  .marker { text-align: center; font-weight: 700; }
  code { padding-left: 8px; font: inherit; }
  .added { background: color-mix(in srgb, var(--low-bg) 75%, transparent); }
  .removed { background: color-mix(in srgb, var(--high-bg) 75%, transparent); }
  .added .marker { color: var(--low); } .removed .marker { color: var(--high); }
  .hunk { color: var(--accent); background: color-mix(in srgb, var(--accent) 9%, var(--panel)); }
  .meta { color: var(--muted); }
  .collapsed { border-block: 1px solid var(--line); background: color-mix(in srgb, var(--accent) 7%, var(--panel)); }
  .collapsed summary { padding: 4px 10px; color: var(--accent); cursor: pointer; user-select: none; }
</style>