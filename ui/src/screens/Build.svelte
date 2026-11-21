<script lang="ts">
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { RiskLevel, UiError } from '../lib/types';

  let { path, onStart }: { path: string; onStart: (jobId: string) => void } = $props();
  let backend = $state<'github' | 'local'>('github');
  let risk = $state<RiskLevel | null>(null);
  let riskLoaded = $state(false);
  let highAccepted = $state(false);
  let mediumConfirmed = $state(false);
  let loading = $state(false);
  let error = $state<UiError | null>(null);

  $effect(() => {
    const local = backend === 'local';
    risk = null;
    riskLoaded = false;
    error = null;
    api.checkBuildRisk(path, local)
      .then((value) => {
        risk = value;
        riskLoaded = true;
      })
      .catch((e: UiError) => (error = e));
  });

  let blocked = $derived(
    backend === 'local' &&
      ((risk === 'high' && !highAccepted) || (risk === 'medium' && !mediumConfirmed))
  );

  async function start() {
    loading = true;
    error = null;
    try {
      const id = await api.startBuild({
        path,
        local: backend === 'local',
        repo: null,
        stages: ['build', 'test', 'server'],
        timeout_secs: 1800,
        java: null,
        workdir: null,
        accept_high_risk: highAccepted,
        confirm_medium: mediumConfirmed,
      });
      onStart(id);
    } catch (e) {
      error = e as UiError;
    } finally {
      loading = false;
    }
  }
</script>

<h1>{t('build.title')}</h1>
<p class="muted path">{path}</p>
<label for="build-backend">{t('build.backend')}</label>
<select id="build-backend" bind:value={backend}>
  <option value="github">{t('build.github')}</option>
  <option value="local">{t('build.local')}</option>
</select>
<h2>{t('build.risk')}</h2>
{#if error}
  <div class="error" role="alert">{error.message}</div>
{:else if !riskLoaded}
  <p role="status">{t('build.scanning')}</p>
{:else}
  <p>{risk ? t(`risk.${risk}`) : t('analysis.riskNone')}</p>
{/if}
{#if backend === 'local' && risk === 'high'}
  <label class="ack"><input type="checkbox" bind:checked={highAccepted} /> {t('build.acceptHigh')}</label>
{:else if backend === 'local' && risk === 'medium'}
  <label class="ack"><input type="checkbox" bind:checked={mediumConfirmed} /> {t('build.confirmMedium')}</label>
{/if}
<p class="muted">{backend === 'local' ? t('build.localWarning') : t('build.githubWarning')}</p>
<button class="primary" onclick={start} disabled={loading || !riskLoaded || blocked}>
  {loading ? t('build.starting') : t('build.start')}
</button>

<style>
  select { min-width: 220px; }
  .ack { display: flex; gap: 8px; align-items: center; margin: 12px 0; }
</style>