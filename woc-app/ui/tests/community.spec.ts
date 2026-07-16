import { expect, test } from '@playwright/test';

const communityUrl = (state: string) => `/?state=${state}&page=community`;

for (const scenario of [
  'community-all-healthy',
  'community-one-feed-failed',
  'community-all-loading',
  'community-cached',
] as const) {
  test(`${scenario} community fixture`, async ({ page }) => {
    const errors: string[] = [];
    page.on('console', (message) => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('pageerror', (error) => errors.push(error.message));

    await page.goto(communityUrl(scenario));
    await expect(page.getByRole('region', { name: 'Community' })).toBeVisible();
    await page.screenshot({ path: `test-results/phase-12-${scenario}.png` });
    expect(errors).toEqual([]);
  });
}

test('one failed feed leaves the other community sections usable', async ({ page }) => {
  await page.goto(communityUrl('community-one-feed-failed'));

  await expect(page.getByText('12.8K')).toBeVisible();
  await expect(page.getByText('Claudemoon', { exact: true })).toBeVisible();
  await expect(page.getByText('Moonweaver', { exact: true })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Community links' })).toBeVisible();
  await expect(page.getByText('Community data is unavailable')).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Retry' })).toHaveCount(1);
  await page.getByRole('button', { name: 'Retry' }).click();
  await expect.poll(() => page.locator('html').getAttribute('data-fixture-community-feed'))
    .toBe('releases');
});

test('release summary skips every Swift metadata prefix', async ({ page }) => {
  await page.goto(communityUrl('community-all-healthy'));

  await expect(page.getByText('New adventures, balance changes, and realm improvements.')).toBeVisible();
  await expect(page.getByText('2 days ago', { exact: true })).toBeVisible();
  await expect(page.getByText('# v0.23.0', { exact: true })).toHaveCount(0);
  await expect(page.getByText('**Release:** v0.23.0', { exact: true })).toHaveCount(0);
  await expect(page.getByText('**Date:** 2026-07-09', { exact: true })).toHaveCount(0);
  await expect(page.getByText('**Previous release:** v0.22.0', { exact: true })).toHaveCount(0);
});

test('leaderboard renders lossy rows and prefers virtual level', async ({ page }) => {
  await page.goto(communityUrl('community-all-healthy'));

  const leaders = page.locator('.leader');
  await expect(leaders).toHaveCount(3);
  await expect(leaders.nth(0)).toContainText('#1MoonweaverMage · Level 94 · 2.8M XP · Prestige 7');
  await expect(leaders.nth(1)).toContainText('#2ClaudeKnightLevel 91 · 2.6M XP');
  await expect(leaders.nth(1)).not.toContainText('Prestige');
  await expect(leaders.nth(2)).toContainText('#3SonnetRanger');
  await expect(leaders.nth(2)).not.toContainText('Level');
});

test('loading and cached fixtures expose their provenance independently', async ({ page }) => {
  await page.goto(communityUrl('community-all-loading'));
  await expect(page.locator('.skeleton')).toHaveCount(2);
  await expect(page.getByText('Loading community data')).toBeVisible();
  await expect(page.locator('.snapshot .metrics')).toContainText('—');
  await expect(page.getByText('Community data is unavailable')).toHaveCount(0);

  await page.goto(communityUrl('community-cached'));
  await expect(page.getByText('updated 2h ago')).toHaveCount(4);
  await expect(page.getByRole('button', { name: 'Retry' })).toHaveCount(4);
  await expect(page.getByText('Moonweaver', { exact: true })).toBeVisible();
});

test('community links use the validated HTTPS fixture transport', async ({ page }) => {
  await page.goto(communityUrl('community-all-healthy'));
  await page.getByRole('button', { name: 'Wiki' }).click();
  await expect.poll(() => page.locator('html').getAttribute('data-fixture-opened-url'))
    .toBe('https://worldofclaudecraft.com/wiki/');
});
