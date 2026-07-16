import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Transport } from "./types";

export const tauriTransport: Transport = {
  invoke: (command, args) => invoke(command, args),
  listen: async <T>(event: string, handler: (payload: T) => void) =>
    listen<T>(event, ({ payload }) => handler(payload)),
};

let activeTransport: Transport = tauriTransport;

export function setTransport(transport: Transport): void {
  activeTransport = transport;
}

export function getTransport(): Transport {
  return activeTransport;
}
