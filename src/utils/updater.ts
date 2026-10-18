// 应用内更新：封装 tauri-plugin-updater，提供「检查」「下载安装」「自动检测开关」。
// 网络健壮性（修 2026-10-14 更新失败）：check 结果缓存复用（避免弹窗后二次 check 遇网络波动），
// 下载失败自动重试一次。
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

const AUTO_KEY = 'gp.autoUpdate';

// last successful check result, reused by downloadAndInstall
let cachedUpdate: Update | null = null;

export function isAutoUpdateOn(): boolean {
  // default ON: fresh installs get auto update unless the user explicitly turned it off
  return localStorage.getItem(AUTO_KEY) !== 'off';
}

export function setAutoUpdate(on: boolean): void {
  localStorage.setItem(AUTO_KEY, on === true ? 'on' : 'off');
}

export interface UpdateInfo {
  version: string;
  current: string;
  body?: string;
  date?: string;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const u = await check();
  cachedUpdate = u;
  if (!u) return null;
  return { version: u.version, current: u.currentVersion, body: u.body, date: u.date };
}

export async function downloadAndInstall(): Promise<void> {
  const u = cachedUpdate ?? (await check());
  cachedUpdate = null;
  if (!u) return;
  try {
    await u.downloadAndInstall();
  } catch (e) {
    // network flaps are common on the GitHub path: refresh via one more check, retry once
    const retry = await check();
    if (!retry) throw e;
    await retry.downloadAndInstall();
  }
  await relaunch();
}
