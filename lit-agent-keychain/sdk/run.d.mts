import type { SpawnOptions } from "node:child_process";

/** Structural view of a child process so tests can substitute a fake. */
export type ChildLike = {
  once(event: string, handler: (...args: any[]) => void): unknown;
  kill(signal?: NodeJS.Signals | number): unknown;
};

export interface RunOptions {
  identityFile: string;
  configFile: string;
  /** Secret names to inject; null injects every export-release secret. */
  only: string[] | null;
  /** Secret name to environment variable name. */
  rename: Record<string, string>;
  /** Secret name to file path; such secrets stay out of the environment unless also renamed. */
  files: Record<string, string>;
  /** Command and arguments after `--`. */
  command: string[];
}

export interface InjectionPlan {
  plan: { name: string; envVar?: string; file?: string }[];
  skipped: string[];
}

export function parseRunArgs(args: string[]): RunOptions;

export function planInjection(
  list: { name: string; operation: string }[],
  options: Pick<RunOptions, "only" | "rename"> &
    Partial<Pick<RunOptions, "files">>,
): InjectionPlan;

export function runWithSecrets(
  client: {
    list(): { name: string; operation: string }[];
    get(name: string): Promise<string>;
    destroy(): void;
  },
  options: Pick<RunOptions, "only" | "rename" | "files" | "command">,
  io: {
    spawn: (file: string, args: string[], options: SpawnOptions) => ChildLike;
    env: NodeJS.ProcessEnv | Record<string, string>;
    stderr: { write(chunk: string): unknown };
    process: {
      on(signal: string, handler: () => void): unknown;
      off(signal: string, handler: () => void): unknown;
    };
  },
): Promise<number>;
