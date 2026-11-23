import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Ai from '../src/screens/Ai.svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const request = {
  input: '/mods/demo', from: '1.20.1', to: '1.21.1',
  loader: 'fabric', min_confidence: 0.8,
};

function mockBackend(saved: Record<string, boolean>) {
  invoke.mockReset();
  invoke.mockImplementation(async (command: string, args?: { provider?: string }) => {
    if (command === 'settings') return { ai_provider: 'anthropic', language: 'en' };
    if (command === 'ai_key_status') {
      const has = saved[args?.provider ?? ''] ?? false;
      return { saved: has, masked: has ? 'sk-…0000' : null, base_url: null };
    }
    if (command === 'ai_key_save') {
      saved[args?.provider ?? ''] = true;
      return { masked: 'sk-…0000', warning: null };
    }
    if (command === 'start_ai_assist') return 'job-1';
    return undefined;
  });
}

describe('AI tab provider selector', () => {
  it('switching provider persists it and shows that provider key form', async () => {
    mockBackend({});
    render(Ai, { request, onStart: vi.fn() });

    const select = await screen.findByLabelText('AI provider');
    await waitFor(() => expect(select).toBeEnabled());
    expect(select).toHaveValue('anthropic');
    await fireEvent.change(select, { target: { value: 'openai' } });

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('save_settings', {
        value: expect.objectContaining({ ai_provider: 'openai' }),
      }),
    );
    expect(await screen.findByText('OpenAI-compatible API key')).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith('ai_key_status', { provider: 'openai' });
  });

  it('a key typed after switching is saved under the chosen provider', async () => {
    mockBackend({});
    render(Ai, { request, onStart: vi.fn() });

    const select = await screen.findByLabelText('AI provider');
    await waitFor(() => expect(select).toBeEnabled());
    await fireEvent.change(select, { target: { value: 'openai' } });
    await screen.findByText('OpenAI-compatible API key');
    await fireEvent.input(screen.getByLabelText('API key'), {
      target: { value: 'sk-proj-test-0000000000000000' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Start AI assist' }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('ai_key_save', expect.objectContaining({ provider: 'openai' })),
    );
    expect(invoke).not.toHaveBeenCalledWith('ai_key_save', expect.objectContaining({ provider: 'anthropic' }));
  });

  it('keeps the old provider and shows the error when saving the choice fails', async () => {
    mockBackend({});
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === 'save_settings') throw { kind: 'io', message: 'disk full' };
      return base(command, args as never);
    });
    render(Ai, { request, onStart: vi.fn() });

    const select = await screen.findByLabelText('AI provider');
    await waitFor(() => expect(select).toBeEnabled());
    await fireEvent.change(select, { target: { value: 'openai' } });

    expect(await screen.findByRole('alert')).toHaveTextContent('disk full');
    expect(select).toHaveValue('anthropic');
    expect(screen.getByText('Anthropic API key (sk-ant-…)')).toBeInTheDocument();
  });
});
