import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Build from '../src/screens/Build.svelte';
import Settings from '../src/screens/Settings.svelte';
import { setLang, translate } from '../src/lib/i18n.svelte';

afterEach(() => setLang('en'));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'check_build_risk') return 'high';
    if (cmd === 'settings') return {
      data_dir: '/tmp/ummp',
      kb_path: null,
      output_root: null,
      sandbox_repo: null,
      ai_provider: 'anthropic',
      language: 'en',
    };
    if (cmd === 'has_secret') return false;
    if (cmd === 'start_build') return 'job-11d';
    return undefined;
  }),
}));

describe('Build risk gate', () => {
  it('requires an explicit checkbox before a High-risk local build', async () => {
    const onStart = vi.fn();
    render(Build, { path: '/mods/demo', onStart });
    await fireEvent.change(screen.getByLabelText('Build backend'), { target: { value: 'local' } });
    const start = screen.getByRole('button', { name: 'Start build' });
    expect(await screen.findByText('High risk')).toBeInTheDocument();
    expect(start).toBeDisabled();
    await fireEvent.click(screen.getByRole('checkbox'));
    expect(start).toBeEnabled();
    await fireEvent.click(start);
    expect(onStart).toHaveBeenCalledWith('job-11d');
  });
});

describe('Settings and localization', () => {
  it('changes visible labels immediately when the language changes', async () => {
    render(Settings);
    expect(await screen.findByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    await fireEvent.change(await screen.findByLabelText('Language'), { target: { value: 'vi' } });
    expect(screen.getByRole('heading', { name: 'Cài đặt' })).toBeInTheDocument();
  });

  it('falls back to English when a Vietnamese translation is missing', () => {
    expect(translate('vi', 'build.passed')).toBe('Passed');
  });
});
