<script lang="ts">
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { PlanRequest, TargetsView, UiError } from '../lib/types';

  let { input, onContinue }: {
    input: string;
    onContinue: (request: PlanRequest, mode: 'port' | 'in_place', output: string | null) => void;
  } = $props();
  let targets = $state<TargetsView | null>(null);
  let version = $state('');
  let loader = $state('');
  let mode = $state<'port' | 'in_place'>('port');
  let output = $state('');
  let error = $state<UiError | null>(null);

  $effect(() => {
    api.targets().then((value) => {
      targets = value;
      version = value.versions[0]?.minecraft ?? '';
      loader = value.versions[0]?.loaders[0] ?? value.loaders[0] ?? '';
    }).catch((e: UiError) => (error = e));
  });

  let availableLoaders = $derived(targets?.versions.find((v) => v.minecraft === version)?.loaders ?? targets?.loaders ?? []);

  function changeVersion(value: string) {
    version = value;
    const loaders = targets?.versions.find((v) => v.minecraft === value)?.loaders ?? [];
    if (!loaders.includes(loader)) loader = loaders[0] ?? '';
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!version || !loader) return;
    onContinue({ input, from: null, to: version, loader, min_confidence: 0.8 }, mode, output.trim() || null);
  }
</script>

<h1>{t('target.title')}</h1>
<p class="muted path">{input}</p>
{#if error}
  <div class="error" role="alert">{error.message}</div>
{:else if !targets}
  <p role="status">{t('target.loading')}</p>
{:else}
  <form onsubmit={submit} class="panel-form">
    <label for="target-version">{t('target.version')}</label>
    <select id="target-version" value={version} onchange={(e) => changeVersion(e.currentTarget.value)} required>
      {#each targets.versions as item (item.minecraft)}
        <option value={item.minecraft}>{item.minecraft}</option>
      {/each}
    </select>

    <label for="target-loader">{t('target.loader')}</label>
    <select id="target-loader" bind:value={loader} required>
      {#each availableLoaders as item (item)}
        <option value={item}>{item}</option>
      {/each}
    </select>

    <fieldset>
      <legend>{t('target.mode')}</legend>
      <label><input type="radio" name="mode" value="port" bind:group={mode} /> {t('target.port')}</label>
      <label><input type="radio" name="mode" value="in_place" bind:group={mode} /> {t('target.inPlace')}</label>
    </fieldset>

    {#if mode === 'port'}
      <label for="target-output">{t('target.output')}</label>
      <input id="target-output" bind:value={output} placeholder={t('target.outputDefault')} />
    {/if}
    <button class="primary" type="submit" disabled={!version || !loader}>{t('target.preview')}</button>
  </form>
{/if}

<style>
  .panel-form { display: grid; gap: 8px; max-width: 560px; }
  .panel-form select, .panel-form > input { width: 100%; box-sizing: border-box; }
  fieldset { display: flex; gap: 18px; border: 1px solid var(--line); border-radius: 5px; margin: 8px 0; padding: 10px; }
  fieldset label { margin: 0; }
  input[type="radio"] { accent-color: var(--accent); }
</style>