import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

const golden = (name: string) =>
  JSON.parse(readFileSync(`../tests/fixtures/golden/app-views/${name}.json`, 'utf8'));

test('Home to Analysis shows the mod and risk from golden JSON', async ({ page }) => {
  const views = { analyze: golden('analysis'), scan: golden('risk'), sessions: golden('sessions') };
  await page.addInitScript((v) => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => (v as Record<string, unknown>)[cmd],
      transformCallback: () => 0,
    };
  }, views);
  await page.goto('/');
  await expect(page.getByText('boom')).toBeVisible();
  await page.getByLabel('Path to a mod').fill('/mods/demo.jar');
  await page.getByRole('button', { name: 'Analyze' }).click();
  await expect(page.getByTestId('mod-id')).toHaveText('demomod');
  await expect(page.getByTestId('risk-badge').first()).toContainText('High risk');
});


test('ZIP sources use archive port mode and disable AI assist', async ({ page }) => {
  await page.addInitScript(() => {
    let portRequest: any = null;
    (window as any).__testPortRequest = () => portRequest;
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string, args: any) => {
        if (cmd === 'sessions') return [];
        if (cmd === 'analyze') return {
          kind: 'zip',
          mods: [{ id: 'demomod', name: 'Demo Mod', version: '1.0', loader: 'fabric', minecraft: '1.20.1', java: 21 }],
          build_system: 'gradle', languages: ['Java'], technologies: [],
        };
        if (cmd === 'scan') return { level: null, findings: [] };
        if (cmd === 'targets') return {
          loaders: ['fabric'],
          versions: [{ minecraft: '1.21.1', loaders: ['fabric'] }],
        };
        if (cmd === 'plan') return {
          source_minecraft: '1.20.1', target_minecraft: '1.21.1',
          source_loader: 'fabric', target_loader: 'fabric',
          files: [], moves: [], deletes: [], technologies: [], skipped: [], warnings: [], files_scanned: 2,
        };
        if (cmd === 'start_port') { portRequest = args.request; return 'job-zip'; }
        if (cmd === 'job') return {
          id: 'job-zip', session: 'session-zip', step: 'scan', state: 'Running',
          progress: null, log: [], result: null, error: null,
        };
        if (cmd === 'plugin:event|listen') return 1;
        if (cmd === 'plugin:event|unlisten') return null;
        return null;
      },
      transformCallback: () => 0,
      unregisterCallback: () => undefined,
    };
  });
  await page.goto('/');
  await page.getByLabel('Path to a mod').fill('/mods/demo.zip');
  await page.getByRole('button', { name: 'Analyze' }).click();
  await expect(page.getByTestId('mod-id')).toHaveText('demomod');
  await page.getByRole('button', { name: 'Choose target' }).click();
  await page.getByLabel('Minecraft version').selectOption('1.21.1');
  await page.getByLabel('Mod loader').selectOption('fabric');
  await page.getByRole('button', { name: 'Preview changes' }).click();
  await expect(page.getByRole('button', { name: 'AI assist' })).toBeDisabled();
  await page.getByRole('button', { name: 'Start port' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__testPortRequest()?.zip)).toBe(true);
});

test('Target → Plan → reviewed in-place Run → Report → Rollback', async ({ page }) => {
  const completedJob = {
    id: 'job-11c',
    session: 'session-11c',
    step: 'done',
    state: 'Succeeded',
    progress: { done: 5, total: 5 },
    log: ['migration completed'],
    result: {
      checkpoint: 'checkpoint-11c',
      report: {
        files_changed: 2, replacements: 3, moves: 1, deletes: 0,
        skipped_mappings: 0, breaking_changes: 0, warnings: [],
        technologies: [], checkpoint_id: 'checkpoint-11c',
      },
    },
    error: null,
  };
  await page.addInitScript(({ job }) => {
    const plan = {
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
      moves: [], deletes: [], technologies: [], skipped: [], warnings: [], files_scanned: 3,
    };
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === 'analyze') return {
          kind: 'folder',
          mods: [{ id: 'demomod', name: 'Demo Mod', version: '1.0', loader: 'fabric', minecraft: '1.20.1', java: 21 }],
          build_system: 'gradle', languages: ['Java'], technologies: [],
        };
        if (cmd === 'scan') return { level: null, findings: [] };
        if (cmd === 'sessions') return [];
        if (cmd === 'targets') return {
          loaders: ['forge', 'fabric', 'neoforge', 'quilt'],
          versions: [{ minecraft: '1.21.1', loaders: ['fabric', 'neoforge'] }],
        };
        if (cmd === 'plan') return plan;
        if (cmd === 'start_migrate_in_place') return 'job-11c';
        if (cmd === 'job') return job;
        if (cmd === 'rollback') return null;
        if (cmd === 'plugin:event|listen') return 1;
        if (cmd === 'plugin:event|unlisten') return null;
        return null;
      },
      transformCallback: () => 0,
      unregisterCallback: () => undefined,
    };
  }, { job: completedJob });
  await page.goto('/');
  await page.screenshot({ path: 'test-results/phase11-home.png', fullPage: true });
  await page.getByLabel('Path to a mod').fill('/mods/demo');
  await page.getByRole('button', { name: 'Analyze' }).click();
  await expect(page.getByTestId('mod-id')).toHaveText('demomod');
  await page.getByRole('button', { name: 'Choose target' }).click();
  await page.getByLabel('Minecraft version').selectOption('1.21.1');
  await page.getByLabel('Mod loader').selectOption('neoforge');
  await page.getByLabel('Migrate in place (checkpoint + rollback)').check();
  await page.getByRole('button', { name: 'Preview changes' }).click();
  await expect(page.getByRole('heading', { name: 'src/main/resources/fabric.mod.json' })).toBeVisible();
  await page.screenshot({ path: 'test-results/phase11-plan.png', fullPage: true });
  const apply = page.getByRole('button', { name: 'Apply reviewed migration' });
  await expect(apply).toBeDisabled();
  await page.getByLabel('I reviewed the changes and understand what will be modified.').check();
  await expect(apply).toBeEnabled();
  await apply.click();
  await expect(page.getByRole('heading', { name: 'Migration report' })).toBeVisible();
  await expect(page.getByText('Files changed')).toBeVisible();
  await page.getByRole('button', { name: 'Roll back in-place changes' }).click();
  await expect(page.getByRole('button', { name: 'Rolled back' })).toBeVisible();
});


test('modpack report paginates 300 rows and records first-page render time', async ({ page }) => {
  const packGolden = golden('pack-report-300');
  const items = packGolden.items;
  const job = {
    id: 'job-pack', session: 'session-pack', step: 'done', state: 'Succeeded',
    progress: { done: 3, total: 3 }, log: [], error: null,
    result: { output: packGolden.output, written: [], manual: 0, items, exit_code: 0 },
  };
  await page.addInitScript(({ completedJob }) => {
    (window as any).__testOpenOutput = false;
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === 'sessions') return [];
        if (cmd === 'targets') return {
          loaders: ['fabric', 'neoforge'],
          versions: [{ minecraft: '1.21.1', loaders: ['fabric', 'neoforge'] }],
        };
        if (cmd === 'start_pack') return 'job-pack';
        if (cmd === 'job') return completedJob;
        if (cmd === 'open_output') { (window as any).__testOpenOutput = true; return null; }
        if (cmd === 'plugin:event|listen') return 1;
        if (cmd === 'plugin:event|unlisten') return null;
        return null;
      },
      transformCallback: () => 0,
      unregisterCallback: () => undefined,
    };
  }, { completedJob: job });
  await page.goto('/');
  await page.getByRole('button', { name: 'Modpack' }).click();
  await page.getByLabel('Modpack file or folder').fill('/packs/demo.mrpack');
  await page.evaluate(() => performance.mark('pack-start'));
  await page.getByRole('button', { name: 'Migrate modpack' }).click();
  await expect(page.getByRole('heading', { name: 'Modpack migration report' })).toBeVisible();
  await page.screenshot({ path: 'test-results/phase11-pack-report.png', fullPage: true });
  await page.getByRole('button', { name: 'Open output folder' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__testOpenOutput)).toBe(true);
  await expect(page.getByRole('cell', { name: 'mod-0', exact: true })).toBeVisible();
  await expect(page.getByText('mod-50')).toHaveCount(0);
  const elapsed = await page.evaluate(() => performance.now() - performance.getEntriesByName('pack-start')[0].startTime);
  test.info().annotations.push({ type: 'pack-first-page-render-ms', description: elapsed.toFixed(1) });
  expect(elapsed).toBeLessThan(2000);
});

test('keyboard focus starts on the Home navigation control and remains visible', async ({ page }) => {
  await page.addInitScript(() => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => cmd === 'sessions' ? [] : null,
      transformCallback: () => 0,
    };
  });
  await page.goto('/');
  await page.keyboard.press('Tab');
  const focused = page.locator(':focus');
  await expect(focused).toHaveRole('button', { name: 'Home' });
  const outline = await focused.evaluate((element) => getComputedStyle(element).outlineStyle);
  expect(outline).not.toBe('none');
});


test('AI approval events require explicit A/R decisions', async ({ page }) => {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (event: unknown) => void>();
    let nextCallback = 1;
    let finished = false;
    let aiStarts = 0;
    const decisions: boolean[] = [];
    (window as any).__testAiStarts = () => aiStarts;
    (window as any).__testDecisions = decisions;
    (window as any).__testEmit = (event: unknown) => {
      for (const callback of callbacks.values()) callback({ payload: event });
    };
    (window as any).__testCallbackCount = () => callbacks.size;
    (window as any).__TAURI_INTERNALS__ = {
      transformCallback: (callback: (event: unknown) => void) => {
        const id = nextCallback++;
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback: (id: number) => callbacks.delete(id),
      invoke: async (cmd: string, args: any) => {
        if (cmd === 'sessions') return [];
        if (cmd === 'analyze') return {
          kind: 'folder',
          mods: [{ id: 'demomod', name: 'Demo Mod', version: '1.0', loader: 'fabric', minecraft: '1.20.1', java: 21 }],
          build_system: 'gradle', languages: ['Java'], technologies: [],
        };
        if (cmd === 'scan') return { level: null, findings: [] };
        if (cmd === 'targets') return {
          loaders: ['fabric', 'neoforge'],
          versions: [{ minecraft: '1.21.1', loaders: ['fabric', 'neoforge'] }],
        };
        if (cmd === 'plan') return {
          source_minecraft: '1.20.1', target_minecraft: '1.21.1',
          source_loader: 'fabric', target_loader: 'neoforge',
          files: [], moves: [], deletes: [], technologies: [], skipped: [], warnings: [], files_scanned: 2,
        };
        if (cmd === 'start_ai_assist') { aiStarts += 1; return 'job-ai'; }
        if (cmd === 'job') return finished ? {
          id: 'job-ai', session: 'session-ai', step: 'done', state: 'Succeeded',
          progress: { done: 4, total: 4 }, log: [], error: null,
          result: { proposed: ['DemoWidget.java'], applied: ['DemoWidget.java'], rejected: 1, checkpoint: null },
        } : {
          id: 'job-ai', session: 'session-ai', step: 'assisting', state: 'Running',
          progress: { done: 3, total: 4 }, log: [], result: null, error: null,
        };
        if (cmd === 'decide') {
          decisions.push(Boolean(args?.approve));
          return null;
        }
        if (cmd === 'plugin:event|listen') return 1;
        if (cmd === 'plugin:event|unlisten') return null;
        return null;
      },
    };
    (window as any).__testFinish = () => {
      finished = true;
      (window as any).__testEmit({
        id: 'event-finished', kind: 'job.finished', timestamp: new Date().toISOString(),
        payload: { job: 'job-ai', state: 'Succeeded', result: null, error: null },
      });
    };
  });
  await page.goto('/');
  await page.getByLabel('Path to a mod').fill('/mods/demo');
  await page.getByRole('button', { name: 'Analyze' }).click();
  await page.getByRole('button', { name: 'Choose target' }).click();
  await page.getByLabel('Minecraft version').selectOption('1.21.1');
  await page.getByLabel('Mod loader').selectOption('neoforge');
  await page.getByRole('button', { name: 'Preview changes' }).click();
  await page.getByRole('button', { name: 'AI assist' }).click();
  await page.getByRole('button', { name: 'Start AI assist' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__testAiStarts())).toBe(1);
  await expect(page.getByRole('heading', { name: 'Migration progress' })).toBeVisible();
  await page.waitForFunction(() => (window as any).__testCallbackCount() > 0);

  const emit = async (approval: string) => page.evaluate((id) => {
    (window as any).__testEmit({
      id: `event-${id}`, kind: 'job.approval', timestamp: new Date().toISOString(),
      payload: {
        job: 'job-ai', approval: id, path: 'src/main/java/DemoWidget.java',
        diff: '--- a/DemoWidget.java\n+++ b/DemoWidget.java\n-old\n+new',
        reviewer_approved: true, concerns: [],
      },
    });
  }, approval);

  await emit('approval-a');
  await expect(page.getByRole('heading', { name: 'Review proposed edit: src/main/java/DemoWidget.java' })).toBeVisible();
  await page.keyboard.press('a');
  await expect.poll(() => page.evaluate(() => (window as any).__testDecisions.length)).toBe(1);

  await emit('approval-r');
  await expect(page.getByRole('heading', { name: 'Review proposed edit: src/main/java/DemoWidget.java' })).toBeVisible();
  await page.keyboard.press('r');
  await expect.poll(() => page.evaluate(() => (window as any).__testDecisions.length)).toBe(2);
  expect(await page.evaluate(() => (window as any).__testDecisions)).toEqual([true, false]);

  await page.evaluate(() => (window as any).__testFinish());
  await expect(page.getByRole('heading', { name: 'Migration report' })).toBeVisible();
});
