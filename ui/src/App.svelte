<script lang="ts">
  import { t, setLang, getLang } from './lib/i18n.svelte';
  import { api } from './lib/api';
  import Home from './screens/Home.svelte';
  import Analysis from './screens/Analysis.svelte';
  import Target from './screens/Target.svelte';
  import Plan from './screens/Plan.svelte';
  import Run from './screens/Run.svelte';
  import Report from './screens/Report.svelte';
  import Build from './screens/Build.svelte';
  import Ai from './screens/Ai.svelte';
  import Pack from './screens/Pack.svelte';
  import PackReport from './screens/PackReport.svelte';
  import Settings from './screens/Settings.svelte';
  import type { JobView, PlanRequest, UiError } from './lib/types';

  type Screen = 'home' | 'analysis' | 'target' | 'plan' | 'run' | 'report' | 'build' | 'ai' | 'pack' | 'pack-report' | 'settings';
  let screen = $state<Screen>('home');
  let input = $state('');
  let request = $state<PlanRequest | null>(null);
  let canAi = $derived(Boolean(request && !request.input.toLowerCase().endsWith('.zip')));
  let mode = $state<'port' | 'in_place'>('port');
  let output = $state<string | null>(null);
  let buildPath = $state('');
  let jobId = $state('');
  let jobKind = $state<'migration' | 'pack'>('migration');
  let finishedJob = $state<JobView | null>(null);
  let error = $state<UiError | null>(null);

  $effect(() => {
    api.settings()
      .then((value) => {
        if (value?.language) setLang(value.language);
      })
      .catch(() => undefined);
  });

  function analyze(path: string) {
    input = path;
    screen = 'analysis';
  }

  function chooseTarget() {
    screen = 'target';
  }

  function preview(next: PlanRequest, nextMode: 'port' | 'in_place', nextOutput: string | null) {
    request = next;
    mode = nextMode;
    output = nextOutput;
    screen = 'plan';
  }

  async function start(next: PlanRequest, nextMode: 'port' | 'in_place', nextOutput: string | null, reviewed: boolean) {
    error = null;
    try {
      jobKind = 'migration';
      jobId = nextMode === 'port'
        ? await api.startPort({
            input: next.input,
            output: nextOutput,
            to: next.to,
            from: next.from,
            loader: next.loader,
            zip: next.input.toLowerCase().endsWith('.zip'),
            min_confidence: next.min_confidence,
          })
        : await api.startMigrateInPlace(next, reviewed);
      finishedJob = null;
      screen = 'run';
    } catch (e) {
      error = e as UiError;
    }
  }

  function startAi(id: string) {
    jobKind = 'migration';
    jobId = id;
    finishedJob = null;
    screen = 'run';
  }

  function startPack(id: string) {
    jobKind = 'pack';
    jobId = id;
    finishedJob = null;
    screen = 'run';
  }

  function startBuild(id: string) {
    jobKind = 'migration';
    jobId = id;
    finishedJob = null;
    screen = 'run';
  }

  function complete(job: JobView) {
    finishedJob = job;
    screen = jobKind === 'pack' ? 'pack-report' : 'report';
  }

  function openBuild(path: string) {
    buildPath = path;
    screen = 'build';
  }
</script>

<div class="shell">
  <nav aria-label="Main">
    <strong class="brand">{t('app.name')}</strong>
    <button class:active={screen === 'home'} onclick={() => (screen = 'home')}>{t('nav.home')}</button>
    <button class:active={screen === 'analysis'} disabled={!input} onclick={() => (screen = 'analysis')}>{t('nav.analysis')}</button>
    <button class:active={screen === 'target' || screen === 'plan'} disabled={!input} onclick={chooseTarget}>{t('nav.target')}</button>
    <button class:active={screen === 'ai'} disabled={!canAi} onclick={() => (screen = 'ai')}>{t('nav.ai')}</button>
    <button class:active={screen === 'pack' || screen === 'pack-report'} onclick={() => (screen = 'pack')}>{t('nav.pack')}</button>
    <button class:active={screen === 'settings'} onclick={() => (screen = 'settings')}>{t('nav.settings')}</button>
    <span class="spacer"></span>
    <select aria-label="Language" value={getLang()} onchange={(e) => setLang(e.currentTarget.value as 'vi' | 'en')}>
      <option value="en">English</option>
      <option value="vi">Tiếng Việt</option>
    </select>
  </nav>
  <main>
    {#if error}<div class="error" role="alert">{t(`error.${error.kind}`)}: {error.message}</div>{/if}
    {#if screen === 'home'}
      <Home onAnalyze={analyze} />
    {:else if screen === 'analysis'}
      <Analysis {input} onContinue={chooseTarget} />
    {:else if screen === 'target'}
      <Target {input} onContinue={preview} />
    {:else if screen === 'plan' && request}
      <Plan {request} {mode} {output} onRun={start} />
    {:else if screen === 'run'}
      <Run {jobId} onFinished={complete} />
    {:else if screen === 'report' && finishedJob}
      <Report job={finishedJob} {input} onBuild={openBuild} />
    {:else if screen === 'build'}
      <Build path={buildPath} onStart={startBuild} />
    {:else if screen === 'ai' && request && canAi}
      <Ai {request} onStart={startAi} />
    {:else if screen === 'pack'}
      <Pack onStart={startPack} />
    {:else if screen === 'pack-report' && finishedJob}
      <PackReport job={finishedJob} />
    {:else if screen === 'settings'}
      <Settings />
    {/if}
  </main>
</div>
