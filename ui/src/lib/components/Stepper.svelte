<script lang="ts">
  import { t } from '../i18n.svelte';
  let { steps, current }: { steps: string[]; current: string } = $props();
</script>

<ol class="stepper" aria-label="Job progress">
  {#each steps as step, i (step)}
    <li class:current={step === current} class:done={steps.indexOf(current) > i}>
      <span>{i + 1}</span>{t(step)}
    </li>
  {/each}
</ol>

<style>
  .stepper { display: flex; flex-wrap: wrap; gap: 8px; list-style: none; padding: 0; }
  li { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  li span { display: grid; place-items: center; width: 22px; height: 22px; border: 1px solid var(--line); border-radius: 50%; }
  li.current { color: var(--accent); font-weight: 700; }
  li.current span, li.done span { border-color: var(--accent); background: var(--accent); color: var(--accent-text); }
</style>