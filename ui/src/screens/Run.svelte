<script lang="ts">
  import { api } from '../lib/api';
  import { onCoreEvent } from '../lib/events';
  import { t } from '../lib/i18n.svelte';
  import DiffView from '../lib/components/DiffView.svelte';
  import JobLog from '../lib/components/JobLog.svelte';
  import Stepper from '../lib/components/Stepper.svelte';
  import type { CoreEvent, JobApprovalView, JobView, UiError } from '../lib/types';

  let { jobId, onFinished }: { jobId: string; onFinished: (job: JobView) => void } = $props();
  let job = $state<JobView | null>(null);
  let approval = $state<JobApprovalView | null>(null);
  let error = $state<UiError | null>(null);
  let stopping = $state(false);
  let deciding = $state(false);
  const steps = ['step.scan', 'step.plan', 'step.port', 'step.apply', 'step.verify', 'step.done'];
  const stepMap: Record<string, string> = {
    scan: 'step.scan', validate: 'step.scan',
    plan: 'step.plan', planning: 'step.plan', analysing: 'step.plan',
    port: 'step.port', porting: 'step.port', migrating: 'step.port',
    apply: 'step.apply', applying: 'step.apply', building: 'step.apply', assisting: 'step.apply',
    review: 'step.apply', awaiting_approval: 'step.apply',
    verify: 'step.verify', done: 'step.done',
  };
  let currentStep = $derived(stepMap[job?.step ?? ''] ?? 'step.scan');

  async function refresh() {
    try {
      job = await api.job(jobId);
      if (job.state !== 'Running' && job.state !== 'Pending') onFinished(job);
    } catch (e) {
      error = e as UiError;
    }
  }

  async function stop() {
    stopping = true;
    try {
      await api.cancel(jobId);
    } catch (e) {
      error = e as UiError;
    }
  }

  async function decide(approve: boolean) {
    if (!approval || deciding) return;
    deciding = true;
    error = null;
    try {
      await api.decide(approval.approval, approve);
      approval = null;
    } catch (e) {
      error = e as UiError;
    } finally {
      deciding = false;
    }
  }

  function handleKey(event: KeyboardEvent) {
    if (!approval || deciding) return;
    const target = event.target;
    if (target instanceof HTMLElement &&
      (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;
    if (event.key.toLowerCase() === 'a') {
      event.preventDefault();
      void decide(true);
    } else if (event.key.toLowerCase() === 'r') {
      event.preventDefault();
      void decide(false);
    }
  }

  $effect(() => {
    let alive = true;
    let unlisten: (() => void) | null = null;
    api.job(jobId).then((value) => {
      if (alive) {
        job = value;
        if (value.state !== 'Running' && value.state !== 'Pending') onFinished(value);
      }
    }).catch((e: UiError) => (error = e));
    onCoreEvent((event: CoreEvent) => {
      const payload = event.payload;
      if (payload.job !== jobId) return;
      if (event.kind === 'job.log' && typeof payload.line === 'string' && job) {
        job = { ...job, log: [...job.log, payload.line].slice(-200) };
      }
      if (event.kind === 'job.progress' && typeof payload.step === 'string') {
        const progress = { done: Number(payload.done ?? 0), total: Number(payload.total ?? 0) };
        if (job) job = { ...job, step: payload.step, progress };
      }
      if (event.kind === 'job.approval' &&
        typeof payload.approval === 'string' &&
        typeof payload.path === 'string' &&
        typeof payload.diff === 'string') {
        approval = {
          job: jobId,
          approval: payload.approval,
          path: payload.path,
          diff: payload.diff,
          reviewer_approved: payload.reviewer_approved === true,
          concerns: Array.isArray(payload.concerns) ? payload.concerns.map(String) : [],
        };
      }
      if (event.kind === 'job.finished') void refresh();
    }).then((stopListening) => {
      if (alive) unlisten = stopListening;
      else stopListening();
    }).catch((e: UiError) => (error = e));
    return () => {
      alive = false;
      unlisten?.();
    };
  });
</script>

<svelte:window onkeydown={handleKey} />
<h1>{t('run.title')}</h1>
{#if error}<div class="error" role="alert">{error.message}</div>{/if}
{#if job}
  <p><strong>{t(`state.${job.state}`)}</strong> · {job.step}</p>
  <Stepper {steps} current={currentStep} />
  {#if job.progress}
    <progress max={job.progress.total || 1} value={job.progress.done}></progress>
    <span>{job.progress.done}/{job.progress.total}</span>
  {/if}
  {#if approval}
    <section class="approval" aria-label={t('ai.approval')}>
      <h2>{t('ai.approval')}: {approval.path}</h2>
      <p>{approval.reviewer_approved ? t('ai.reviewerApproved') : t('ai.reviewerRejected')}</p>
      {#if approval.concerns.length}
        <ul>{#each approval.concerns as concern (concern)}<li>{concern}</li>{/each}</ul>
      {/if}
      <DiffView diff={approval.diff} />
      <div class="decision">
        <button class="primary" onclick={() => decide(true)} disabled={deciding}>{t('ai.approve')} (A)</button>
        <button class="danger" onclick={() => decide(false)} disabled={deciding}>{t('ai.reject')} (R)</button>
      </div>
    </section>
  {/if}
  <JobLog lines={job.log} />
  {#if job.state === 'Running' || job.state === 'Pending'}
    <button class="danger" onclick={stop} disabled={stopping}>{stopping ? t('run.stopping') : t('run.cancel')}</button>
  {:else}
    <p role="status">{job.state === 'Succeeded' ? t('run.finished') : t('run.ended')}</p>
  {/if}
{:else}
  <p role="status">{t('run.loading')}</p>
{/if}

<style>
  progress { width: min(520px, 75%); accent-color: var(--accent); margin-right: 10px; }
  .danger { margin-top: 12px; border-color: var(--high); color: var(--high); }
  .approval { margin: 14px 0; padding: 12px; border: 1px solid var(--accent); border-radius: 5px; background: var(--panel); }
  .approval h2 { overflow-wrap: anywhere; }
  .decision { display: flex; gap: 8px; margin-top: 10px; }
</style>