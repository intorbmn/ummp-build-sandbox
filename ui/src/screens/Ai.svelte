<script lang="ts">
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { PlanRequest, UiError } from '../lib/types';

  let { request, onStart }: { request: PlanRequest; onStart: (jobId: string) => void } = $props();
  let loading = $state(false);
  let error = $state<UiError | null>(null);

  async function start() {
    loading = true;
    error = null;
    try {
      onStart(await api.startAiAssist(request));
    } catch (e) {
      error = e as UiError;
    } finally {
      loading = false;
    }
  }
</script>

<h1>{t('ai.title')}</h1>
<p>{t('ai.description')}</p>
<dl>
  <dt>{t('target.version')}</dt><dd>{request.to}</dd>
  <dt>{t('target.loader')}</dt><dd>{request.loader}</dd>
</dl>
<p class="muted">{t('ai.approvalNote')}</p>
{#if error}<div class="error" role="alert">{error.message}</div>{/if}
<button class="primary" onclick={start} disabled={loading}>
  {loading ? t('ai.starting') : t('ai.start')}
</button>