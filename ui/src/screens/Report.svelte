<script lang="ts">
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { JobView, ReportView, UiError } from '../lib/types';

  let { job, input, onBuild }: { job: JobView; input: string; onBuild: (path: string) => void } = $props();
  let rollbackError = $state<UiError | null>(null);
  let openError = $state<UiError | null>(null);
  let rolledBack = $state(false);

  let report = $derived((job.result?.report && typeof job.result.report === 'object'
    ? job.result.report : null) as ReportView | null);
  let buildReport = $derived(typeof job.result?.report === 'string' ? job.result.report : null);
  let isBuild = $derived(typeof job.result?.passed === 'boolean');
  let isAi = $derived(Array.isArray(job.result?.proposed));
  let proposedCount = $derived(Array.isArray(job.result?.proposed) ? job.result.proposed.length : 0);
  let appliedCount = $derived(Array.isArray(job.result?.applied) ? job.result.applied.length : 0);
  let output = $derived(typeof job.result?.output === 'string'
    ? job.result.output
    : typeof job.result?.workdir === 'string' ? job.result.workdir : null);
  let checkpoint = $derived(
    report?.checkpoint_id ??
    (typeof job.result?.checkpoint === 'string' ? job.result.checkpoint : null)
  );

  async function openOutput() {
    openError = null;
    try {
      await api.openOutput(job.id);
    } catch (e) {
      openError = e as UiError;
    }
  }

  async function rollback() {
    rollbackError = null;
    try {
      await api.rollback(input, checkpoint);
      rolledBack = true;
    } catch (e) {
      rollbackError = e as UiError;
    }
  }
</script>

<h1>{t('report.title')}</h1>
<p><strong>{t(`state.${job.state}`)}</strong> · {job.step}</p>
{#if job.error}<div class="error" role="alert">{job.error}</div>{/if}

{#if report}
  <section class="metrics">
    <div><strong>{report.files_changed}</strong><span>{t('report.changed')}</span></div>
    <div><strong>{report.replacements}</strong><span>{t('report.replacements')}</span></div>
    <div><strong>{report.moves}</strong><span>{t('report.moves')}</span></div>
    <div><strong>{report.deletes}</strong><span>{t('report.deletes')}</span></div>
    <div><strong>{report.skipped_mappings}</strong><span>{t('report.skipped')}</span></div>
    <div><strong>{report.breaking_changes}</strong><span>{t('report.breaking')}</span></div>
  </section>
  {#if report.warnings.length}
    <h2>{t('plan.warnings')}</h2>
    <ul>{#each report.warnings as warning (warning)}<li>{warning}</li>{/each}</ul>
  {/if}
  {#if report.technologies.length}
    <h2>{t('plan.technologies')}</h2>
    <ul>{#each report.technologies as finding, i (`${finding.tech}-${finding.file}-${i}`)}
      <li><strong>{finding.tech}</strong> · {t(`handling.${finding.handling}`)} · {finding.message} <code>{finding.file}</code></li>
    {/each}</ul>
  {/if}
{:else if isBuild}
  <section class="metrics">
    <div><strong>{job.result?.passed ? t('build.passed') : t('build.failed')}</strong><span>{t('build.status')}</span></div>
    <div><strong>{String(job.result?.stages_passed ?? 0)}/{String(job.result?.stages_run ?? 0)}</strong><span>{t('build.stages')}</span></div>
  </section>
  {#if buildReport}<h2>{t('build.report')}</h2><pre>{buildReport}</pre>{/if}
{:else if isAi}
  <h2>{t('ai.summary')}</h2>
  <p>{t('ai.proposed')}: {proposedCount} · {t('ai.applied')}: {appliedCount} · {t('ai.rejected')}: {String(job.result?.rejected ?? 0)}</p>
{:else}
  <p class="muted">{t('report.noMigrationReport')}</p>
{/if}

{#if output}
  <p><strong>{t('report.output')}:</strong> <code>{output}</code></p>
  <button onclick={openOutput}>{t('report.openOutput')}</button>
  {#if !isBuild}<button onclick={() => onBuild(output)}>{t('report.build')}</button>{/if}
{/if}
{#if openError}<p class="error" role="alert">{openError.message}</p>{/if}
{#if checkpoint}
  <button class="danger" onclick={rollback} disabled={rolledBack}>{rolledBack ? t('report.rolledBack') : t('report.rollback')}</button>
{/if}
{#if rollbackError}<p class="error" role="alert">{rollbackError.message}</p>{/if}

<style>
  .metrics { display: grid; grid-template-columns: repeat(auto-fit, minmax(115px, 1fr)); gap: 8px; margin: 14px 0; }
  .metrics div { display: flex; flex-direction: column; gap: 3px; padding: 10px; background: var(--panel); border: 1px solid var(--line); border-radius: 5px; }
  .metrics strong { font-size: 20px; } .metrics span { color: var(--muted); }
  .danger { margin-left: 8px; color: var(--high); border-color: var(--high); }
  pre { max-height: 50vh; overflow: auto; padding: 10px; border: 1px solid var(--line); background: var(--panel); white-space: pre-wrap; }
</style>