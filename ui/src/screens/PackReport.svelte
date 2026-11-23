<script lang="ts">
  import { t } from '../lib/i18n.svelte';
  import { api } from '../lib/api';
  import type { UiError } from '../lib/types';
  import type { JobView, PackItemView } from '../lib/types';

  let { job }: { job: JobView } = $props();
  let page = $state(0);
  let openError = $state<UiError | null>(null);
  const pageSize = 50;
  let items = $derived((job.result?.items ?? []) as PackItemView[]);
  let pages = $derived(Math.max(1, Math.ceil(items.length / pageSize)));
  let visible = $derived(items.slice(page * pageSize, (page + 1) * pageSize));
  let output = $derived(typeof job.result?.output === 'string' ? job.result.output : null);

  async function openOutput() {
    openError = null;
    try {
      await api.openOutput(job.id);
    } catch (e) {
      openError = e as UiError;
    }
  }
</script>

<h1>{t('pack.report')}</h1>
<p>{items.length} {t('pack.mods')} · {t(`state.${job.state}`)}</p>
{#if job.error}<div class="error" role="alert">{job.error}</div>{/if}
{#if output}
  <p><strong>{t('report.output')}:</strong> <code>{output}</code></p>
  <button onclick={openOutput}>{t('report.openOutput')}</button>
{/if}
{#if openError}<p class="error" role="alert">{openError.message}</p>{/if}
<div class="table-wrap">
  <table>
    <thead><tr><th>{t('pack.modId')}</th><th>{t('pack.action')}</th><th>{t('pack.reason')}</th><th>{t('pack.path')}</th></tr></thead>
    <tbody>
      {#each visible as item (item.path)}
        <tr>
          <td>{item.mod_id}</td>
          <td>{item.action.action}</td>
          <td>{item.reason}</td>
          <td><code>{item.path}</code></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
<div class="pagination">
  <button onclick={() => (page = Math.max(0, page - 1))} disabled={page === 0}>{t('pack.previous')}</button>
  <span>{page + 1}/{pages}</span>
  <button onclick={() => (page = Math.min(pages - 1, page + 1))} disabled={page + 1 >= pages}>{t('pack.next')}</button>
</div>

<style>
  .table-wrap { overflow: auto; max-height: 62vh; border: 1px solid var(--line); border-radius: 5px; }
  table { min-width: 720px; }
  td { vertical-align: top; }
  .pagination { display: flex; gap: 12px; align-items: center; justify-content: flex-end; margin-top: 10px; }
</style>