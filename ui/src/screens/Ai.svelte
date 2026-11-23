<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { AiKeyStatus, PlanRequest, SettingsView, UiError } from '../lib/types';

  let { request, onStart }: { request: PlanRequest; onStart: (jobId: string) => void } = $props();
  let loading = $state(true);
  let error = $state<UiError | null>(null);
  let provider = $state('anthropic');
  let status = $state<AiKeyStatus | null>(null);
  let key = $state('');
  let baseUrl = $state('');
  let editing = $state(false);
  let showKey = $state(false);
  let warning = $state('');

  let settings: SettingsView | null = null;

  async function refresh() {
    status = await api.aiKeyStatus(provider);
    baseUrl = status.base_url ?? '';
  }

  // The backend starts AI with the provider stored in settings, so the choice is saved
  // before anything else; on failure the previous provider stays selected.
  async function changeProvider(select: HTMLSelectElement) {
    const next = select.value;
    if (next === provider || !settings) return;
    loading = true;
    error = null;
    try {
      await api.saveSettings({ ...settings, ai_provider: next });
      settings = { ...settings, ai_provider: next };
      provider = next;
      key = '';
      editing = false;
      warning = '';
      await refresh();
    } catch (e) {
      error = e as UiError;
      select.value = provider;
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    try {
      settings = await api.settings();
      provider = settings.ai_provider ?? 'anthropic';
      await refresh();
    } catch (e) {
      error = e as UiError;
    } finally {
      loading = false;
    }
  });

  async function start() {
    loading = true;
    error = null;
    warning = '';
    try {
      if (!status?.saved || editing) {
        if (!key.trim()) {
          error = { kind: 'input', message: t('ai.keyRequired') };
          return;
        }
        const saved = await api.aiKeySave(provider, key, provider === 'openai' ? baseUrl : null);
        key = '';
        editing = false;
        warning = saved.warning ?? '';
        await refresh();
      }
      onStart(await api.startAiAssist(request));
    } catch (e) {
      error = e as UiError;
    } finally {
      loading = false;
    }
  }

  async function removeKey() {
    loading = true;
    error = null;
    try {
      await api.aiKeyDelete(provider);
      key = '';
      editing = false;
      await refresh();
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

<section class="credential">
  <label for="ai-provider">{t('settings.provider')}</label>
  <select id="ai-provider" value={provider} disabled={loading} onchange={(e) => changeProvider(e.currentTarget)}>
    <option value="anthropic">Anthropic</option>
    <option value="openai">OpenAI</option>
  </select>
  <h2>{provider === 'openai' ? t('ai.openaiKey') : t('ai.anthropicKey')}</h2>
  {#if status?.saved && !editing}
    <p><code>{status.masked}</code></p>
    <button onclick={() => (editing = true)} disabled={loading}>{t('ai.changeKey')}</button>
    <button onclick={removeKey} disabled={loading}>{t('ai.deleteKey')}</button>
  {:else}
    <label for="ai-key">{t('ai.keyInput')}</label>
    <div class="key-input">
      <input id="ai-key" type={showKey ? 'text' : 'password'} autocomplete="off" bind:value={key} />
      <button type="button" onclick={() => (showKey = !showKey)}>{showKey ? t('ai.hideKey') : t('ai.showKey')}</button>
    </div>
    {#if provider === 'openai'}
      <label for="ai-base-url">{t('ai.baseUrl')}</label>
      <input id="ai-base-url" type="url" bind:value={baseUrl} placeholder="https://api.openai.com/v1" />
    {/if}
    {#if status?.saved}
      <button onclick={() => { editing = false; key = ''; }} disabled={loading}>{t('ai.cancelChange')}</button>
    {/if}
  {/if}
  {#if warning}<p class="muted" role="status">{warning}</p>{/if}
</section>

{#if error}<div class="error" role="alert">{error.message}</div>{/if}
<button class="primary" onclick={start} disabled={loading}>
  {loading ? t('ai.starting') : t('ai.start')}
</button>

<style>
  .credential { display: grid; gap: 8px; max-width: 560px; margin: 18px 0; }
  .key-input { display: flex; gap: 8px; }
  .key-input input { flex: 1; min-width: 0; }
</style>
