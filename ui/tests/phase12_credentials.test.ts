import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Ai from '../src/screens/Ai.svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

describe('shared AI credentials', () => {
  it('shows the key field when no key is saved', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'settings') return { ai_provider: 'anthropic' };
      if (command === 'ai_key_status') {
        return { saved: false, masked: null, base_url: null };
      }
      return undefined;
    });

    render(Ai, {
      request: {
        input: '/mods/demo', from: '1.20.1', to: '1.21.1',
        loader: 'fabric', min_confidence: 0.8,
      },
      onStart: vi.fn(),
    });

    expect(await screen.findByLabelText('API key')).toBeInTheDocument();
  });

  it('saves the typed key before starting AI and clears the input', async () => {
    const onStart = vi.fn();
    let saved = false;
    invoke.mockImplementation(async (command: string) => {
      if (command === 'settings') return { ai_provider: 'anthropic' };
      if (command === 'ai_key_status') {
        return {
          saved,
          masked: saved ? 'sk-ant-…0000' : null,
          path: '/tmp/credentials.json',
          base_url: null,
        };
      }
      if (command === 'ai_key_save') {
        saved = true;
        return { masked: 'sk-ant-…0000', warning: null };
      }
      if (command === 'start_ai_assist') return 'job-1';
      return undefined;
    });

    render(Ai, {
      request: {
        input: '/mods/demo', from: '1.20.1', to: '1.21.1',
        loader: 'fabric', min_confidence: 0.8,
      },
      onStart,
    });

    const input = await screen.findByLabelText('API key');
    await fireEvent.input(input, { target: { value: 'sk-ant-test-0000000000000000' } });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Start AI assist' })).toBeEnabled(),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Start AI assist' }));

    await waitFor(() => expect(onStart).toHaveBeenCalledWith('job-1'));
    const saveIndex = invoke.mock.calls.findIndex(([command]) => command === 'ai_key_save');
    const startIndex = invoke.mock.calls.findIndex(([command]) => command === 'start_ai_assist');
    expect(saveIndex).toBeGreaterThanOrEqual(0);
    expect(startIndex).toBeGreaterThan(saveIndex);
    expect(input).toHaveValue('');
    expect(await screen.findByText('sk-ant-…0000')).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent('sk-ant-test-0000000000000000');
  });
});
