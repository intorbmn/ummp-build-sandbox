<script lang="ts">
  import type { FileDiffView, PlanView } from '../types';
  let { plan, selected, onSelect }: {
    plan: PlanView;
    selected: string;
    onSelect: (file: FileDiffView) => void;
  } = $props();
</script>

<div class="tree" aria-label="Changed files">
  <strong>{plan.files.length} changed</strong>
  {#each plan.files as file (file.path)}
    <button class:selected={selected === file.path} class="file" onclick={() => onSelect(file)}>
      <span aria-hidden="true">{file.kind === 'created' ? '+' : '±'}</span>
      <span class="path">{file.path}</span>
      {#if file.needs_attention}<span class="attention" title="Needs review">!</span>{/if}
    </button>
  {/each}
  {#each plan.moves as move (move.from)}
    <div class="file muted"><span>→</span><span class="path">{move.from} → {move.to}</span></div>
  {/each}
  {#each plan.deletes as path (path)}
    <div class="file removed"><span>−</span><span class="path">{path}</span></div>
  {/each}
  {#if plan.files.length + plan.moves.length + plan.deletes.length === 0}
    <p class="muted">No file changes in this plan.</p>
  {/if}
</div>

<style>
  .tree { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .tree > strong { padding: 4px 8px; color: var(--muted); }
  .file { display: flex; gap: 7px; align-items: center; width: 100%; min-width: 0; text-align: left; border: 0; border-radius: 3px; background: transparent; padding: 5px 8px; }
  .file.selected { background: color-mix(in srgb, var(--accent) 14%, var(--panel)); outline: 1px solid var(--accent); }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .attention { margin-left: auto; color: var(--medium); font-weight: 800; }
  .removed { color: var(--high); }
</style>