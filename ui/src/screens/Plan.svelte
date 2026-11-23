<script lang="ts">
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import DiffView from '../lib/components/DiffView.svelte';
  import FileTree from '../lib/components/FileTree.svelte';
  import type { FileDiffView, PlanRequest, PlanView, UiError } from '../lib/types';

  let { request, mode, output, onRun }: {
    request: PlanRequest;
    mode: 'port' | 'in_place';
    output: string | null;
    onRun: (request: PlanRequest, mode: 'port' | 'in_place', output: string | null, reviewed: boolean) => void;
  } = $props();
  let plan = $state<PlanView | null>(null);
  let selected = $state<FileDiffView | null>(null);
  let reviewed = $state(false);
  let error = $state<UiError | null>(null);

  $effect(() => {
    const current = request;
    plan = null;
    selected = null;
    reviewed = false;
    error = null;
    api.plan(current).then((value) => {
      plan = value;
      selected = value.files[0] ?? null;
    }).catch((e: UiError) => (error = e));
  });
</script>

<h1>{t('plan.title')}</h1>
<p class="muted">{request.to} · {request.loader} · {mode === 'port' ? t('target.port') : t('target.inPlace')}</p>
{#if error}
  <div class="error" role="alert"><strong>{t(`error.${error.kind}`)}</strong> {error.message}</div>
{:else if !plan}
  <p role="status">{t('plan.loading')}</p>
{:else}
  <section class="summary" aria-label="Plan summary">
    <span>{plan.files.length} {t('plan.files')}</span>
    <span>{plan.moves.length} {t('plan.moves')}</span>
    <span>{plan.deletes.length} {t('plan.deletes')}</span>
    <span>{plan.files_scanned} {t('plan.scanned')}</span>
  </section>
  {#if plan.warnings.length}
    <section class="warnings" aria-label="Warnings">
      <strong>{t('plan.warnings')}</strong>
      <ul>{#each plan.warnings as warning (warning)}<li>{warning}</li>{/each}</ul>
    </section>
  {/if}
  {#if plan.technologies.length}
    <section class="findings">
      <strong>{t('plan.technologies')}</strong>
      {#each plan.technologies as finding, i (`${finding.tech}-${finding.file}-${i}`)}
        <p><span class="handling" data-handling={finding.handling}>{t(`handling.${finding.handling}`)}</span>
          <strong>{finding.tech}</strong> · {finding.message} <code>{finding.file}{finding.line ? `:${finding.line}` : ''}</code></p>
      {/each}
    </section>
  {/if}
  {#if plan.skipped.length}
    <details class="skipped">
      <summary>{t('plan.skipped')} ({plan.skipped.length})</summary>
      <ul>{#each plan.skipped as item (`${item.description}-${item.reason}`)}
        <li><strong>{item.description}</strong>: {item.reason}</li>
      {/each}</ul>
    </details>
  {/if}
  <div class="workspace">
    <aside><FileTree {plan} selected={selected?.path ?? ''} onSelect={(file) => (selected = file)} /></aside>
    <section class="preview">
      {#if selected}
        <h2>{selected.path}</h2>
        {#if selected.needs_attention}<p class="attention">{t('plan.attention')}</p>{/if}
        <DiffView diff={selected.diff} />
      {:else}
        <p class="muted">{t('plan.noDiff')}</p>
      {/if}
    </section>
  </div>
  {#if mode === 'in_place'}
    <label class="review"><input type="checkbox" bind:checked={reviewed} /> {t('plan.reviewed')}</label>
  {/if}
  <button class="primary" disabled={mode === 'in_place' && !reviewed}
    onclick={() => onRun(request, mode, output, reviewed)}>{mode === 'port' ? t('plan.startPort') : t('plan.apply')}</button>
{/if}

<style>
  .summary { display: flex; flex-wrap: wrap; gap: 14px; padding: 9px 12px; margin: 10px 0; background: var(--panel); border: 1px solid var(--line); border-radius: 5px; }
  .workspace { display: grid; grid-template-columns: minmax(220px, 30%) minmax(0, 1fr); gap: 12px; margin: 12px 0; }
  .workspace aside, .preview { min-width: 0; padding: 8px; border: 1px solid var(--line); border-radius: 5px; background: var(--panel); }
  .preview h2 { overflow-wrap: anywhere; }
  .warnings { padding: 8px 12px; border-left: 3px solid var(--medium); background: var(--medium-bg); }
  .warnings ul { margin: 5px 0; }
  .attention { color: var(--medium); font-weight: 700; }
  .findings, .skipped { margin: 10px 0; padding: 9px 12px; border: 1px solid var(--line); border-radius: 5px; background: var(--panel); }
  .findings p { margin: 7px 0; overflow-wrap: anywhere; }
  .handling { font-size: 11px; font-weight: 700; text-transform: uppercase; color: var(--muted); margin-right: 7px; }
  .handling[data-handling="manual"] { color: var(--high); }
  .handling[data-handling="assisted"] { color: var(--medium); }
  .skipped summary { cursor: pointer; }
  .review { display: flex; align-items: center; gap: 8px; margin: 12px 0; }
  @media (max-width: 720px) { .workspace { grid-template-columns: 1fr; } }
</style>