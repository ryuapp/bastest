export interface RuntimeLineReader extends AsyncIterable<string> {
  close(): void;
}

export interface RuntimePlugin {
  now(): number;
  log(message: string): void;
  args(): string[];
  readLines(): RuntimeLineReader;
  readTextFile(path: string): string | undefined;
  createDir(path: string): void;
  writeTextFile(path: string, content: string): void;
  removeFile(path: string): void;
  getEnv(name: string): string | undefined;
}

export function defineRuntimePlugin(plugin: RuntimePlugin): RuntimePlugin {
  return plugin;
}
