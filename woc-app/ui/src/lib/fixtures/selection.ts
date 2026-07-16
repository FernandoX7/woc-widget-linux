import { dashboardPages, fixtureScenarios, type DashboardPage, type FixtureScenario } from './types';

export interface FixtureSelection {
  scenario: FixtureScenario;
  page: DashboardPage;
}

export function isFixtureMode(
  search = typeof window === 'undefined' ? '' : window.location.search,
  env: Record<string, string | boolean | undefined> = import.meta.env,
): boolean {
  const params = new URLSearchParams(search);
  return (
    params.has('state') ||
    params.has('page') ||
    params.has('woc-preview-state') ||
    params.has('woc-preview-page') ||
    Boolean(env.VITE_WOC_PREVIEW_STATE) ||
    Boolean(env.VITE_WOC_PREVIEW_PAGE)
  );
}

function pick<T extends string>(value: string | null | undefined, values: readonly T[], fallback: T): T {
  const normalized = value?.trim().toLowerCase();
  return values.includes(normalized as T) ? (normalized as T) : fallback;
}

export function resolveFixtureSelection(
  search = typeof window === 'undefined' ? '' : window.location.search,
  env: Record<string, string | boolean | undefined> = import.meta.env,
): FixtureSelection {
  const params = new URLSearchParams(search);
  return {
    scenario: pick(
      params.get('state') ?? params.get('woc-preview-state') ?? String(env.VITE_WOC_PREVIEW_STATE ?? ''),
      fixtureScenarios,
      'live',
    ),
    page: pick(
      params.get('page') ?? params.get('woc-preview-page') ?? String(env.VITE_WOC_PREVIEW_PAGE ?? ''),
      dashboardPages,
      'overview',
    ),
  };
}
