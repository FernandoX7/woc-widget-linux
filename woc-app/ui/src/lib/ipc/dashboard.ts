import { getTransport } from "./transport";
import {
  IPC_COMMANDS,
  IPC_EVENTS,
  type DashboardPage,
  type DashboardSnapshot,
  type CommunityFeedName,
  type Unlisten,
} from "./types";

export const getDashboardSnapshot = (): Promise<DashboardSnapshot> =>
  getTransport().invoke(IPC_COMMANDS.snapshot);

export const refreshPage = (page: DashboardPage): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.refreshPage, { page });

export const refreshStatus = (): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.refreshStatus);

export const quitApp = (): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.quit);

export const setChartRange = (seconds: number): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.setChartRange, { seconds });

export const dismissWelcome = (): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.dismissWelcome);

export const openExternal = (url: string): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.openExternal, { url });

export const setCandleInterval = (seconds: number): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.setCandleInterval, { seconds });

export const refreshCommunityIfNeeded = (): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.refreshCommunityIfNeeded);

export const refreshCommunityFeed = (feed: CommunityFeedName): Promise<void> =>
  getTransport().invoke(IPC_COMMANDS.refreshCommunityFeed, { feed });

export async function listenForDashboardChanges(
  handler: () => void,
): Promise<Unlisten[]> {
  return Promise.all([
    getTransport().listen(IPC_EVENTS.status, handler),
    getTransport().listen(IPC_EVENTS.market, handler),
    getTransport().listen(IPC_EVENTS.community, handler),
    getTransport().listen(IPC_EVENTS.settings, handler),
  ]);
}

export type { DashboardPage, DashboardSnapshot, Transport } from "./types";
