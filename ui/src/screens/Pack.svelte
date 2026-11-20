<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { TargetsView, UiError } from '../lib/types';

  let { onStart }: { onStart: (jobId: string) => void } = $props();
  let targets = $state<TargetsView | null>(null);
  let input = $state('');
  let sources = $state('');
  let output = $state('');
  let version = $state('');
  let loader = $state('');
  let online = $state(false);
  let allowRisk = $state(false);
  let loading = $state(false);
  let error = $state<UiError | null>(null);

  $effect(() => {
    api.targets().then((value) => {
      targets = value;
      version = value.versions[0]?.minecraft ?? '';
      loader = value.versions[0]?.loaders[0] ?? value.loaders[0] ?? '';
    }).catch((e: UiError) => (error = e));
  });

  let loaders = $derived(targets?.versions.find((v) => v.minecraft === version)?.loaders ?? targets?.loaders ?? []);

  function changeVersion(value: string) {
    version = value;
    const supported = targets?.versions.find((v) => v.minecraft === value)?.loaders ?? [];
    if (!supported.includes(loader)) loader = supported[0] ?? '';
  }

  async function browsePack() {
    const chosen = await open({
      directory: false,
      multiple: false,
      filters: [{ name: 'Modpack', extensions: ['mrpack', 'zip'] }],
    });
    if (typeof chosen === 'string') input = chosen;
  }

  async function browseSources() {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === 'string') sources = chosen;
  }

  async function start(e: SubmitEvent) {
    e.preventDefault();
    loading = true;
    error = null;
    try {
      onStart(await api.startPack({
        input,
        to: version,
        loader,
        from_loader: null,
        from_mc: null,
        online,
        sources: sources.trim() || null,
        side: 'both',
        output: output.trim() || null,
        allow_risk: allowRisk,
        loader_version: null,
        formats: ['mrpack', 'folder'],
        min_confidence: 0.8,
      }));
    } catch (e) {
      error = e as UiError;
    } finally {
      loading = false;
    }
  }
</script>

<h1>{t('pack.title')}</h1>
{#if error}<div class="error" role="alert">{error.message}</div>{/if}
<form class="pack-form" onsubmit={start}>
  <label for="pack-input">{t('pack.input')}</label>
  <div class="row"><input id="pack-input" bind:value={input} required /><button type="button" onclick={browsePack}>{t('pack.browse')}</button></div>
  <label for="pack-version">{t('target.version')}</label>
  <select id="pack-version" value={version} onchange={(e) => changeVersion(e.currentTarget.value)} required>
    {#each targets?.versions ?? [] as item (item.minecraft)}<option value={item.minecraft}>{item.minecraft}</option>{/each}
  </select>
  <label for="pack-loader">{t('target.loader')}</label>
  <select id="pack-loader" bind:value={loader} required>
    {#each loaders as item (item)}<option value={item}>{item}</option>{/each}
  </select>
  <label for="pack-sources">{t('pack.sources')}</label>
  <div class="row"><input id="pack-sources" bind:value={sources} /><button type="button" onclick={browseSources}>{t('pack.browseFolder')}</button></div>
  <label for="pack-output">{t('target.output')}</label>
  <input id="pack-output" bind:value={output} />
  <label class="check"><input type="checkbox" bind:checked={online} /> {t('pack.online')}</label>
  <label class="check"><input type="checkbox" bind:checked={allowRisk} /> {t('pack.allowRisk')}</label>
  <button class="primary" type="submit" disabled={loading || !targets || !input.trim() || !version || !loader}>
    {loading ? t('pack.starting') : t('pack.start')}
  </button>
</form>

<style>
  .pack-form { display: grid; gap: 7px; max-width: 700px; }
  .pack-form > input, .pack-form > select { width: 100%; box-sizing: border-box; }
  .row { display: flex; gap: 6px; } .row input { flex: 1; min-width: 0; }
  .check { display: flex; align-items: center; gap: 8px; margin: 4px 0; }
</style>