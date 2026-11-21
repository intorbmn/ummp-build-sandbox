import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import PackReport from '../src/screens/PackReport.svelte';
import Run from '../src/screens/Run.svelte';

const { onCoreEvent } = vi.hoisted(() => ({
  onCoreEvent: vi.fn(async (handler: (event: any) => void) => {
    handler({
      id: 'event-approval',
      kind: 'job.approval',
      timestamp: '2026-11-20T00:00:00Z',
      payload: {
        job: 'job-ai',
        approval: 'approval-1',
        path: 'src/main/java/DemoWidget.java',
        diff: '--- a/DemoWidget.java\n+++ b/DemoWidget.java\n-old\n+new',
        reviewer_approved: false,
        concerns: ['Check the method signature'],
      },
    });
    return () => undefined;
  }),
}));

vi.mock('../src/lib/events', () => ({ onCoreEvent }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'job') return {
      id: 'job-ai', session: 'session-ai', step: 'assisting', state: 'Running',
      progress: { done: 3, total: 4 }, log: [], result: null, error: null,
    };
    if (cmd === 'decide') return undefined;
    return undefined;
  }),
}));

describe('AI approval', () => {
  it('shows the proposed diff and lets the A key explicitly approve it', async () => {
    render(Run, { jobId: 'job-ai', onFinished: vi.fn() });
    expect(await screen.findByText('Review proposed edit: src/main/java/DemoWidget.java')).toBeInTheDocument();
    expect(screen.getByText('Check the method signature')).toBeInTheDocument();
    await fireEvent.keyDown(window, { key: 'a' });
    const { invoke } = await import('@tauri-apps/api/core');
    expect(invoke).toHaveBeenCalledWith('decide', { approval: 'approval-1', approve: true });
  });
});

describe('Pack report pagination', () => {
  it('renders 50 of 300 rows at a time', async () => {
    const golden = JSON.parse(readFileSync('../tests/fixtures/golden/app-views/pack-report-300.json', 'utf8'));
    const items = golden.items;
    render(PackReport, {
      job: {
        id: 'job-pack', session: 'session-pack', step: 'done', state: 'Succeeded',
        progress: null, log: [], error: null, result: { output: golden.output, items },
      },
    });
    const table = screen.getByRole('table');
    expect(within(table).getAllByRole('row')).toHaveLength(51);
    expect(within(table).getByText('mod-0')).toBeInTheDocument();
    expect(within(table).queryByText('mod-50')).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(within(table).getByText('mod-50')).toBeInTheDocument();
    expect(within(table).queryByText('mod-0')).not.toBeInTheDocument();
  });
});
