// 应用内更新：封装 tauri-plugin-updater，提供「检查」「下载安装」「自动检测开关」。
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

const AUTO_KEY = 'gp.autoUpdate';

export function isAutoUpdateOn(): boolean {
  return localStorage.getItem(AUTO_KEY) === 'on';
}

export function setAutoUpdate(on: boolean): void {
  localStorage.setItem(AUTO_KEY, on ? 'on' : 'off');
}

export interface UpdateInfo {
  version: string;
  current: string;
  body?: string;
  date?: string;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const u = await check();
  if (!u) return null;
  return { version: u.version, current: u.currentVersion, body: u.body, date: u.date };
}

export async function downloadAndInstall(): Promise<void> {
  const u = await check();
  if (!u) return;
  await u.downloadAndInstall();
  await relaunch();
}
