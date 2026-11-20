import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import analysis from '../../tests/fixtures/golden/app-views/analysis.json';
import risk from '../../tests/fixtures/golden/app-views/risk.json';
import Analysis from '../src/screens/Analysis.svelte';
import RiskBadge from '../src/lib/RiskBadge.svelte';
import { translate } from '../src/lib/i18n.svelte';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => (cmd === 'analyze' ? analysis : risk)),
}));

describe('Analysis', () => {
  it('shows mod id and loader from analyze result', async () => {
    render(Analysis, { input: '/mods/demo.jar' });
    expect(await screen.findByTestId('mod-id')).toHaveTextContent('demomod');
    expect(screen.getByTestId('mod-loader')).toHaveTextContent('forge');
  });
});

describe('RiskBadge', () => {
  it('shows text and icon for High', () => {
    render(RiskBadge, { level: 'high' });
    const badge = screen.getByTestId('risk-badge');
    expect(badge).toHaveTextContent('High risk');
    expect(badge).toHaveTextContent('✕');
  });
});

describe('i18n', () => {
  it('falls back to English when a Vietnamese key is missing', () => {
    expect(translate('vi', 'nav.home')).toBe('Trang chủ');
    expect(translate('vi', 'no.such.key')).toBe('no.such.key');
  });
});
