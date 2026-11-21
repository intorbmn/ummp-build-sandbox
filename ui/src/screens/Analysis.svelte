<script lang="ts">
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import RiskBadge from '../lib/RiskBadge.svelte';
  import type { AnalysisView, RiskView, UiError } from '../lib/types';

  let { input, onContinue }: { input: string; onContinue: () => void } = $props();
  let analysis = $state<AnalysisView | null>(null);
  let risk = $state<RiskView | null>(null);
  let error = $state<UiError | null>(null);

  $effect(() => {
    const path = input;
    analysis = null;
    risk = null;
    error = null;
    Promise.all([api.analyze(path), api.scan(path)])
      .then(([a, r]) => {
        if (path === input) {
          analysis = a;
          risk = r;
        }
      })
      .catch((e: UiError) => (error = e));
  });
</script>

<h1>{t('analysis.title')}</h1>
<p class="muted path">{input}</p>

{#if error}
  <div class="error" role="alert">
    <strong>{t(`error.${error.kind}`)}</strong>
    <span>{error.message}</span>
  </div>
{:else if !analysis || !risk}
  <p role="status">{t('analysis.loading')}</p>
{:else}
  <h2>{t('analysis.mods')}</h2>
  <table>
    <thead>
      <tr>
        <th>{t('analysis.col.id')}</th>
        <th>{t('analysis.col.name')}</th>
        <th>{t('analysis.col.version')}</th>
        <th>{t('analysis.col.loader')}</th>
        <th>{t('analysis.col.minecraft')}</th>
        <th>{t('analysis.col.java')}</th>
      </tr>
    </thead>
    <tbody>
      {#each analysis.mods as m (m.id)}
        <tr>
          <td data-testid="mod-id">{m.id}</td>
          <td>{m.name}</td>
          <td>{m.version}</td>
          <td data-testid="mod-loader">{m.loader}</td>
          <td>{m.minecraft}</td>
          <td>{m.java ?? '–'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <dl>
    <dt>{t('analysis.build')}</dt>
    <dd>{analysis.build_system ?? t('analysis.none')}</dd>
    <dt>{t('analysis.languages')}</dt>
    <dd>{analysis.languages.join(', ') || t('analysis.none')}</dd>
  </dl>

  <h2>{t('analysis.technologies')}</h2>
  {#if analysis.technologies.length}
    <ul class="technologies">
      {#each analysis.technologies as finding, i (`${finding.tech}-${finding.file}-${i}`)}
        <li>
          <span class="handling" data-handling={finding.handling}>{t(`handling.${finding.handling}`)}</span>
          <strong>{finding.tech}</strong>
          <span>{finding.message}</span>
          <code>{finding.file}{finding.line ? `:${finding.line}` : ''}</code>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="muted">{t('analysis.none')}</p>
  {/if}

  <h2>{t('analysis.risk')}</h2>
  {#if risk.level}
    <RiskBadge level={risk.level} />
    <table>
      <thead>
        <tr><th>{t('analysis.findings')}</th><th></th><th></th></tr>
      </thead>
      <tbody>
        {#each risk.findings as f, i (i)}
          <tr>
            <td>{f.rule}</td>
            <td>{f.file}</td>
            <td><RiskBadge level={f.severity} /></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p>{t('analysis.riskNone')}</p>
  {/if}
  {#if analysis.kind === 'jar'}
    <p class="muted">{t('analysis.jarOnly')}</p>
  {:else}
    <button class="primary" onclick={onContinue}>{t('analysis.continue')}</button>
  {/if}
{/if}
