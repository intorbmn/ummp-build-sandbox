<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { SessionView } from '../lib/types';

  let { onAnalyze }: { onAnalyze: (path: string) => void } = $props();
  let path = $state('');
  let sessions = $state<SessionView[]>([]);

  $effect(() => {
    api.sessions(10).then((s) => (sessions = s)).catch(() => (sessions = []));
  });

  async function browse(directory: boolean) {
    const chosen = await open({
      directory,
      multiple: false,
      filters: directory ? undefined : [{ name: 'Mod source or JAR', extensions: ['jar', 'zip'] }],
    });
    if (typeof chosen === 'string') path = chosen;
  }

  function submit(e: Event) {
    e.preventDefault();
    if (path.trim()) onAnalyze(path.trim());
  }
</script>

<h1>{t('home.title')}</h1>
<form onsubmit={submit}>
  <label for="mod-path">{t('home.path')}</label>
  <div class="row">
    <input id="mod-path" bind:value={path} />
    <button type="button" onclick={() => browse(true)}>{t('home.pickFolder')}</button>
    <button type="button" onclick={() => browse(false)}>{t('home.pickFile')}</button>
    <button type="submit" class="primary" disabled={!path.trim()}>{t('home.analyze')}</button>
  </div>
</form>

<h2>{t('home.recent')}</h2>
{#if sessions.length === 0}
  <p class="muted">{t('home.recentEmpty')}</p>
{:else}
  <ul class="sessions">
    {#each sessions as s (s.id)}
      <li>
        <span class="state state-{s.state}">{t(`state.${s.state}`)}</span>
        <span>{s.output ?? s.error ?? s.step}</span>
      </li>
    {/each}
  </ul>
{/if}
