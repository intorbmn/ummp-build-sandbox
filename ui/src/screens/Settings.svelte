<script lang="ts">
  import { api } from '../lib/api';
  import { setLang, t } from '../lib/i18n.svelte';
  import type { AiKeyStatus, SettingsView, UiError } from '../lib/types';

  type AiProvider = 'anthropic' | 'openai';
  let form = $state<SettingsView | null>(null);
  let outputRoot = $state('');
  let kbPath = $state('');
  let sandboxRepo = $state('');
  let provider = $state('anthropic');
  let language = $state<'vi' | 'en'>('en');
  let githubToken = $state('');
  let githubSaved = $state(false);
  let aiStatus = $state<Record<AiProvider, AiKeyStatus | null>>({
    anthropic: null, openai: null,
  });
  let aiKeys = $state<Record<AiProvider, string>>({ anthropic: '', openai: '' });
  let editingAi = $state<Record<AiProvider, boolean>>({ anthropic: false, openai: false });
  let aiBaseUrls = $state<Record<AiProvider, string>>({ anthropic: '', openai: '' });
  let error = $state<UiError | null>(null);
  let message = $state('');

  $effect(() => {
    api.settings().then(async (value) => {
      form = value;
      outputRoot = value.output_root ?? '';
      kbPath = value.kb_path ?? '';
      sandboxRepo = value.sandbox_repo ?? '';
      provider = value.ai_provider ?? 'anthropic';
      language = value.language;
      setLang(value.language);
      const [anthropic, openai, github] = await Promise.all([
        api.aiKeyStatus('anthropic'),
        api.aiKeyStatus('openai'),
        api.hasSecret('github_token'),
      ]);
      aiStatus = { anthropic, openai };
      aiBaseUrls = { anthropic: '', openai: openai.base_url ?? '' };
      githubSaved = github;
    }).catch((e: UiError) => (error = e));
  });

  async function save() {
    if (!form) return;
    error = null;
    message = '';
    try {
      await api.saveSettings({
        ...form,
        output_root: outputRoot.trim() || null,
        kb_path: kbPath.trim() || null,
        sandbox_repo: sandboxRepo.trim() || null,
        ai_provider: provider,
        language,
      });
      setLang(language);
      message = t('settings.saved');
    } catch (e) {
      error = e as UiError;
    }
  }

  async function saveAiKey(name: AiProvider) {
    error = null;
    message = '';
    if (!aiKeys[name].trim()) return;
    try {
      const saved = await api.aiKeySave(name, aiKeys[name], name === 'openai' ? aiBaseUrls.openai : null);
      aiKeys[name] = '';
      editingAi[name] = false;
      aiStatus[name] = await api.aiKeyStatus(name);
      message = saved.warning ?? t('settings.secretSaved');
    } catch (e) {
      error = e as UiError;
    }
  }

  async function deleteAiKey(name: AiProvider) {
    error = null;
    try {
      await api.aiKeyDelete(name);
      aiKeys[name] = '';
      editingAi[name] = false;
      aiStatus[name] = await api.aiKeyStatus(name);
    } catch (e) {
      error = e as UiError;
    }
  }

  async function saveGithubToken() {
    error = null;
    try {
      if (!githubToken.trim()) return;
      await api.setSecret('github_token', githubToken);
      githubToken = '';
      githubSaved = true;
      message = t('settings.secretSaved');
    } catch (e) {
      error = e as UiError;
    }
  }

  async function deleteGithubToken() {
    error = null;
    try {
      await api.deleteSecret('github_token');
      githubToken = '';
      githubSaved = false;
    } catch (e) {
      error = e as UiError;
    }
  }
</script>

<h1>{t('settings.title')}</h1>
{#if error}<div class="error" role="alert">{error.message}</div>{/if}
{#if message}<p role="status">{message}</p>{/if}
{#if form}
  <section class="settings">
    <label for="output-root">{t('settings.output')}</label>
    <input id="output-root" bind:value={outputRoot} />
    <label for="kb-path">{t('settings.kb')}</label>
    <input id="kb-path" bind:value={kbPath} />
    <label for="sandbox-repo">{t('settings.repo')}</label>
    <input id="sandbox-repo" bind:value={sandboxRepo} placeholder="owner/repository" />
    <label for="provider">{t('settings.provider')}</label>
    <select id="provider" bind:value={provider}>
      <option value="anthropic">Anthropic</option>
      <option value="openai">OpenAI</option>
    </select>
    <label for="language">{t('settings.language')}</label>
    <select id="language" value={language} onchange={(e) => {
      language = e.currentTarget.value as 'vi' | 'en';
      setLang(language);
    }}>
      <option value="en">English</option>
      <option value="vi">Tiếng Việt</option>
    </select>
    <p class="muted">{t('settings.dataDir')}: <code>{form.data_dir}</code></p>
    <button class="primary" onclick={save}>{t('settings.save')}</button>
  </section>

  <h2>{t('settings.credentials')}</h2>
  <p class="muted">{t('settings.secretHelp')}</p>
  {#each ['anthropic', 'openai'] as name (name)}
    <section class="credential">
      <strong>{name === 'anthropic' ? 'Anthropic' : 'OpenAI'}</strong>
      {#if aiStatus[name as AiProvider]?.saved && !editingAi[name as AiProvider]}
        <code>{aiStatus[name as AiProvider]?.masked}</code>
        <button onclick={() => (editingAi[name as AiProvider] = true)}>{t('ai.changeKey')}</button>
        <button onclick={() => deleteAiKey(name as AiProvider)}>{t('settings.deleteSecret')}</button>
      {:else}
        <input type="password" autocomplete="off" aria-label={name + ' API key'} bind:value={aiKeys[name as AiProvider]} placeholder={t('settings.secretPlaceholder')} />
        {#if name === 'openai'}
          <input type="url" aria-label="OpenAI base URL" bind:value={aiBaseUrls.openai} placeholder="https://api.openai.com/v1" />
        {/if}
        <button onclick={() => saveAiKey(name as AiProvider)} disabled={!aiKeys[name as AiProvider].trim()}>{t('settings.storeSecret')}</button>
      {/if}
    </section>
  {/each}
  <section class="credential">
    <strong>GitHub token</strong>
    <span class="muted">{githubSaved ? t('settings.stored') : t('settings.notStored')}</span>
    <input type="password" autocomplete="off" aria-label="GitHub token" bind:value={githubToken} placeholder={t('settings.secretPlaceholder')} />
    <button onclick={saveGithubToken} disabled={!githubToken.trim()}>{t('settings.storeSecret')}</button>
    <button onclick={deleteGithubToken} disabled={!githubSaved}>{t('settings.deleteSecret')}</button>
  </section>
{:else}
  <p role="status">{t('settings.loading')}</p>
{/if}

<style>
  .settings { display: grid; gap: 7px; max-width: 620px; }
  .settings input, .settings select { width: 100%; box-sizing: border-box; }
  .settings .muted { overflow-wrap: anywhere; }
  .credential { display: grid; grid-template-columns: minmax(110px, 1fr) minmax(150px, 2fr) auto auto; gap: 8px; align-items: center; padding: 9px 0; border-bottom: 1px solid var(--line); }
  @media (max-width: 760px) { .credential { grid-template-columns: 1fr 1fr; } }
</style>
