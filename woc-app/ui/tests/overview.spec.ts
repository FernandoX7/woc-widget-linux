import { expect, test } from '@playwright/test';

const hasCyanSeriesPixels = (selector = '.chart canvas') => Array.from(document.querySelectorAll<HTMLCanvasElement>(selector)).some((canvas) => {
  const context = canvas.getContext('2d');
  if (!context) return false;
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  for (let offset = 0; offset < pixels.length; offset += 4) {
    if (pixels[offset] < 100 && pixels[offset + 1] > 150 && pixels[offset + 2] > 170 && pixels[offset + 3] > 0) return true;
  }
  return false;
});

const scenarios = [
  'live',
  'loading',
  'cached-offline',
  'empty-history',
  'gap-history',
  'rhythm-29',
  'rhythm-30',
] as const;

for (const scenario of scenarios) {
  test(`${scenario} fixture renders without browser errors`, async ({ page }) => {
    const errors: string[] = [];
    page.on('console', (message) => {
      if (message.type() === 'error') errors.push(message.text());
    });
    page.on('pageerror', (error) => errors.push(error.message));

    await page.goto(`/?state=${scenario}&page=overview`);
    await expect(page.getByRole('heading', { name: 'PLAYERS OVER TIME' })).toBeVisible();
    await expect(page.getByText('CLAUDEMOON')).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(150);
    await page.screenshot({ path: `test-results/phase-10-${scenario}.png` });
    expect(errors).toEqual([]);
  });
}

test('range changes select backend-shaped fixture output', async ({ page }) => {
  await page.goto('/?state=live&page=overview');
  for (const [range, resolution] of [['1h', '1m averages'], ['6h', '5m averages'], ['24h', '5m averages'], ['7d', '1h averages']] as const) {
    await page.getByRole('button', { name: range, exact: true }).click();
    await expect(page.getByText(resolution)).toBeVisible();
    await expect(page.getByRole('button', { name: range, exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('.chart canvas').first()).toBeVisible();
    await expect(page.locator('.visual-series .line').first()).toBeVisible();
  }
});

test('history arriving after the initial loading snapshot creates a visible chart', async ({ page }) => {
  await page.goto('/?state=loading&page=overview');
  await expect(page.getByText('Loading player history')).toBeVisible();
  await page.keyboard.press('Control+R');
  await expect(page.getByRole('button', { name: /Players online over 6h/ })).toBeVisible();
  const canvas = page.locator('.chart canvas').first();
  await expect(canvas).toBeVisible();
  await expect.poll(async () => canvas.evaluate((element) => {
    const box = element.getBoundingClientRect();
    return box.width > 0 && box.height > 0;
  })).toBe(true);
  await expect.poll(() => page.evaluate(hasCyanSeriesPixels)).toBe(true);
  await expect(page.locator('.visual-series .line').first()).toBeVisible();
});

test('chart teardown and repeated updates retain one live resize observer', async ({ page }) => {
  await page.addInitScript(() => {
    const NativeResizeObserver = window.ResizeObserver;
    let active = 0;
    class TrackedResizeObserver extends NativeResizeObserver {
      private connected = false;
      observe(target: Element, options?: ResizeObserverOptions) {
        if (!this.connected) { active += 1; this.connected = true; }
        super.observe(target, options);
      }
      disconnect() {
        if (this.connected) { active -= 1; this.connected = false; }
        super.disconnect();
      }
    }
    window.ResizeObserver = TrackedResizeObserver;
    Object.defineProperty(window, '__activeResizeObservers', { get: () => active });
  });
  await page.goto('/?state=live&page=overview');
  await expect(page.locator('.chart canvas').first()).toBeVisible();
  const initial = await page.evaluate(() => (window as Window & { __activeResizeObservers: number }).__activeResizeObservers);
  expect(initial).toBeGreaterThan(0);
  for (let index = 0; index < 3; index += 1) {
    await page.getByRole('button', { name: 'Refresh' }).click();
  }
  expect(await page.evaluate(() => (window as Window & { __activeResizeObservers: number }).__activeResizeObservers)).toBe(initial);
  await page.getByRole('button', { name: 'Community', exact: true }).click();
  await expect(page.locator('.chart canvas')).toHaveCount(0);
  expect(await page.evaluate(() => (window as Window & { __activeResizeObservers: number }).__activeResizeObservers)).toBe(0);
  await page.getByRole('button', { name: 'Overview', exact: true }).click();
  await expect(page.locator('.chart canvas').first()).toBeVisible();
  expect(await page.evaluate(() => (window as Window & { __activeResizeObservers: number }).__activeResizeObservers)).toBe(initial);
  await page.reload();
  await expect(page.locator('.chart canvas').first()).toBeVisible();
});

test('keyboard and pointer chart inspection clear with Escape', async ({ page }) => {
  await page.goto('/?state=live&page=overview');
  const plot = page.getByRole('button', { name: /Players online over 6h/ });
  await plot.focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('.tooltip')).toBeVisible();
  await expect(page.locator('.tooltip span')).toHaveText(/^[A-Z][a-z]{2} \d{1,2} \d{2}:\d{2}$/);
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('.tooltip')).toBeVisible();
  const alignmentError = await page.evaluate(() => {
    const tooltip = document.querySelector<HTMLElement>('.tooltip')!;
    const rule = document.querySelector<SVGLineElement>('.selection-line')!;
    const rulePoint = new DOMPoint(Number(rule.getAttribute('x1')), 0).matrixTransform(rule.getScreenCTM()!);
    const tooltipBox = tooltip.getBoundingClientRect();
    return Math.abs(rulePoint.x - (tooltipBox.left + tooltipBox.width / 2));
  });
  expect(alignmentError).toBeLessThan(1);
  await page.keyboard.press('Escape');
  await expect(page.locator('.tooltip')).toBeHidden();

  const box = await plot.boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await expect(page.locator('.tooltip')).toBeVisible();
});

test('loading, gap, and rhythm boundary fixtures expose the required states', async ({ page }) => {
  await page.goto('/?state=loading&page=overview');
  await expect(page.getByText('Loading player history')).toBeVisible();
  await expect(page.getByRole('button', { name: '1h' })).toBeDisabled();

  await page.goto('/?state=gap-history&page=overview');
  const gapPlot = page.getByRole('button', { name: /Players online over 6h/ });
  await expect(gapPlot).toBeVisible();
  await expect(gapPlot).toHaveAttribute('data-segment-count', '3');
  await expect(gapPlot).toHaveAttribute('data-isolated-count', '1');
  await expect(page.locator('.visual-series .line')).toHaveCount(2);
  await expect(page.locator('.isolated-dot')).toHaveCount(1);
  await expect(page.locator('.visual-series text')).toHaveText(['07:00', '09:02', '11:05']);
  const area = page.locator('.visual-series .area').first();
  const gradientId = await area.getAttribute('fill');
  expect(gradientId).toMatch(/^url\(#player-area-\d+\)$/);
  await expect(page.locator(`${gradientId!.replace('url(', '').replace(')', '')}`)).toHaveCount(1);

  await page.goto('/?state=rhythm-29&page=overview');
  await expect(page.getByText('Gathering observations (29/30)')).toBeVisible();
  await page.goto('/?state=rhythm-30&page=overview');
  await expect(page.getByText(/Busier than \d+% of your observed window/)).toBeVisible();
});

test('dashboard retains discoverable Refresh and Quit actions and shortcuts', async ({ page }) => {
  await page.goto('/?state=live&page=overview');

  const refresh = page.getByRole('button', { name: 'Refresh' });
  const quit = page.getByRole('button', { name: 'Quit' });
  await expect(refresh).toHaveAttribute('title', 'Refresh (Ctrl+R)');
  await expect(quit).toHaveAttribute('title', 'Quit (Ctrl+Q)');

  await refresh.click();
  await expect(page.locator('html')).toHaveAttribute('data-fixture-refreshed-page', 'overview');
  await quit.click();
  await expect(page.locator('html')).toHaveAttribute('data-fixture-quit-requested', 'true');

  await page.evaluate(() => {
    delete document.documentElement.dataset.fixtureRefreshedPage;
    delete document.documentElement.dataset.fixtureQuitRequested;
  });
  await page.keyboard.press('Control+R');
  await expect(page.locator('html')).toHaveAttribute('data-fixture-refreshed-page', 'overview');
  await page.keyboard.press('Control+Q');
  await expect(page.locator('html')).toHaveAttribute('data-fixture-quit-requested', 'true');
});
