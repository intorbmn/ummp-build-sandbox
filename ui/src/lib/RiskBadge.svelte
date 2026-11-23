<script lang="ts">
  import type { RiskLevel } from './types';
  import { t } from './i18n.svelte';

  let { level }: { level: RiskLevel } = $props();
  // Text + icon + colour: colour is never the only signal (WCAG AA).
  const icon = { low: '✓', medium: '!', high: '✕' } as const;
</script>

<span class="badge risk-{level}" data-testid="risk-badge">
  <span aria-hidden="true" class="icon">{icon[level]}</span>
  {t(`risk.${level}`)}
</span>
