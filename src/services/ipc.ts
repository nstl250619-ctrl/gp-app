// 唯一 invoke 封装：点号命令名 -> snake_case（Tauri 命令名约定）。
// 参数按原样传递：Tauri v2 命令参数按 camelCase 匹配 Rust 的 snake_case 形参，
// 前端契约本就是 camelCase，转换反而会丢参（修复：tokenId/modelIds/recordId 类参数报 missing）。
import { invoke } from '@tauri-apps/api/core';
import type { CommandError, CommandResult } from '@/types/ipc';
import { t } from '@/i18n';

export class IpcError extends Error {
  readonly code: CommandError['code'];
  readonly category: CommandError['category'];
  readonly nextAction: string | undefined;
  readonly detail: string | undefined;

  constructor(error: CommandError) {
    super(error.message);
    this.name = 'IpcError';
    this.code = error.code;
    this.category = error.category;
    this.nextAction = error.nextAction;
    this.detail = error.detail;
  }
}

function toSnakeCommand(command: string): string {
  return command
    .replace(/\./g, '_')
    .replace(/([A-Z])/g, (_, ch: string) => `_${ch.toLowerCase()}`);
}

export async function invokeCommand<TData>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<TData> {
  const result = await invoke<CommandResult<TData>>(
    toSnakeCommand(command),
    args,
  );
  if (!result.ok) {
    const error: CommandError = result.error ?? {
      code: 'UNKNOWN',
      category: 'server',
      message: t('common.unknownError'),
    };
    throw new IpcError(error);
  }
  return result.data as TData;
}

export function isIpcError(value: unknown): value is IpcError {
  return value instanceof IpcError;
}

export function errorMessage(value: unknown): string {
  if (value instanceof Error) return value.message;
  if (typeof value === 'string') return value;
  return t('common.unknownError');
}
