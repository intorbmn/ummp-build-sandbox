import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import DiffView from '../src/lib/components/DiffView.svelte';
import Plan from '../src/screens/Plan.svelte';

const planView = {
  source_minecraft: '1.20.1',
  target_minecraft: '1.21.1',
  source_loader: 'fabric',
  target_loader: 'neoforge',
  files: [{
    path: 'src/main/resources/fabric.mod.json',
    kind: 'modified',
    diff: '--- a/fabric.mod.json\n+++ b/fabric.mod.json\n@@ -1 +1 @@\n-old\n+new',
    replacements: 1,
    needs_attention: false,
  }],
  moves: [],
  deletes: [],
  technologies: [],
  skipped: [],
  warnings: [],
  files_scanned: 3,
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'plan') return planView;
    return undefined;
  }),
}));

describe('DiffView', () => {
  it('marks added and removed lines with text markers', () => {
    render(DiffView, { diff: '--- a/file\n+++ b/file\n-old\n+new' });
    expect(screen.getByText('−')).toBeInTheDocument();
    expect(screen.getByText('+')).toBeInTheDocument();
    expect(screen.getByText('-old')).toBeInTheDocument();
    expect(screen.getByText('+new')).toBeInTheDocument();
  });

  it('collapses long unchanged runs and preserves access to the hidden lines', async () => {
    const context = Array.from({ length: 12 }, (_, i) => ` context-${i}`).join('\n');
    render(DiffView, { diff: `@@ -1,14 +1,14 @@\n${context}\n-old\n+new` });
    const hidden = screen.getByText('6 unchanged lines hidden');
    expect(hidden).toBeInTheDocument();
    const line = screen.getByText('context-3');
    expect(line).not.toBeVisible();
    await fireEvent.click(hidden);
    expect(line).toBeVisible();
  });
});

describe('Plan', () => {
  it('keeps in-place apply disabled until the review confirmation is checked', async () => {
    const onRun = vi.fn();
    render(Plan, {
      request: { input: '/mods/demo', from: '1.20.1', to: '1.21.1', loader: 'neoforge', min_confidence: 0.8 },
      mode: 'in_place',
      output: null,
      onRun,
    });
    const apply = await screen.findByRole('button', { name: 'Apply reviewed migration' });
    expect(apply).toBeDisabled();
    await fireEvent.click(screen.getByRole('checkbox'));
    expect(apply).toBeEnabled();
    await fireEvent.click(apply);
    expect(onRun).toHaveBeenCalledWith(
      expect.objectContaining({ to: '1.21.1' }),
      'in_place',
      null,
      true,
    );
  });
});
