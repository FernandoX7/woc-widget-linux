import { getTransport } from "./transport";
import { IPC_COMMANDS, type SettingsSnapshot } from "./types";

export const getSettings = ():Promise<SettingsSnapshot> => getTransport().invoke(IPC_COMMANDS.settingsSnapshot);
export const updateSetting = (key:string,value:unknown):Promise<SettingsSnapshot> => getTransport().invoke(IPC_COMMANDS.updateSetting,{key,value});
export const clearAlertMute = (ruleId:string):Promise<SettingsSnapshot> => getTransport().invoke(IPC_COMMANDS.clearAlertMute,{ruleId});
export const getAutostart = ():Promise<boolean> => getTransport().invoke(IPC_COMMANDS.autostartEnabled);
export const setAutostart = (enabled:boolean):Promise<boolean> => getTransport().invoke(IPC_COMMANDS.setAutostartEnabled,{enabled});
export const sendTestNotification = ():Promise<number> => getTransport().invoke(IPC_COMMANDS.sendTestNotification);
export async function exportHistory(format:"csv"|"json"):Promise<boolean>{
  return getTransport().invoke(IPC_COMMANDS.exportHistory,{format});
}
export const clearHistory = ():Promise<SettingsSnapshot> => getTransport().invoke(IPC_COMMANDS.clearHistory);
export const getHistoryPersistenceError = ():Promise<string|null> => getTransport().invoke(IPC_COMMANDS.historyPersistenceError);
