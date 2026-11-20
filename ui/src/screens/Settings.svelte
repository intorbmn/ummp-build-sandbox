<script lang="ts">
  import { api } from '../lib/api';
  import { setLang, t } from '../lib/i18n.svelte';
  import type { SettingsView, UiError } from '../lib/types';

  type SecretName = 'anthropic_api_key' | 'openai_api_key' | 'github_token';
  let form = $state<SettingsView | null>(null);
  let outputRoot = $state('');
  let kbPath = $state('');
  let sandboxRepo = $state('');
  let provider = $state('anthropic');
  let language = $state<'vi' | 'en'>('en');
  let values = $state<Record<SecretName, string>>({
    anthropic_api_key: '', openai_api_key: '', github_token: '',
  });
  const secretNames: SecretName[] = ['anthropic_api_key', 'openai_api_key', 'github_token'];
  let saved = $state<Record<SecretName, boolean>>({
    anthropic_api_key: false, openai_api_key: false, github_token: false,
  });
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
      const names: SecretName[] = ['anthropic_api_key', 'openai_api_key', 'github_token'];
      const statuses = await Promise.all(names.map((name) => api.hasSecret(name)));
      saved = {
        anthropic_api_key: statuses[0],
        openai_api_key: statuses[1],
        github_token: statuses[2],
      };
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

  async function saveSecret(name: SecretName) {
    error = null;
    message = '';
    if (!values[name].trim()) return;
    try {
      await api.setSecret(name, values[name]);
      values[name] = '';
      saved[name] = true;
      message = t('settings.secretSaved');
    } catch (e) {
      error = e as UiError;
    }
  }

  async function removeSecret(name: SecretName) {
    error = null;
    try {
      await api.deleteSecret(name);
      saved[name] = false;
      values[name] = '';
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
  {#each secretNames as name (name)}
    <section class="credential">
      <div><strong>{name}</strong><span class="muted">{saved[name] ? t('settings.stored') : t('settings.notStored')}</span>
        {#if name === 'github_token'}<small class="muted">{t('settings.githubTokenHelp')}</small>{/if}
      </div>
      <input type="password" autocomplete="off" aria-label={`${name} value`} bind:value={values[name]} placeholder={t('settings.secretPlaceholder')} />
      <button onclick={() => saveSecret(name)} disabled={!values[name].trim()}>{t('settings.storeSecret')}</button>
      <button onclick={() => removeSecret(name)} disabled={!saved[name]}>{t('settings.deleteSecret')}</button>
    </section>
  {/each}
{:else}
  <p role="status">{t('settings.loading')}</p>
{/if}

<style>
  .settings { display: grid; gap: 7px; max-width: 620px; }
  .settings input, .settings select { width: 100%; box-sizing: border-box; }
  .settings .muted { overflow-wrap: anywhere; }
  .credential { display: grid; grid-template-columns: minmax(150px, 1fr) minmax(180px, 2fr) auto auto; gap: 8px; align-items: center; padding: 9px 0; border-bottom: 1px solid var(--line); }
  .credential > div { display: flex; flex-direction: column; }
  @media (max-width: 760px) { .credential { grid-template-columns: 1fr 1fr; } }
</style>