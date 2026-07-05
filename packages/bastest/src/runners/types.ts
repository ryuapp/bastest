export interface RunnerOptions {
  worker: boolean;
  cwd?: string;
  file?: string;
  bundleFile?: string;
  filter?: string;
}

export interface RunFileOptions {
  cwd?: string;
  file: string;
  bundleFile: string;
  filter?: string;
  stream?: boolean;
}

export interface WorkerRequest extends RunnerOptions {
  file: string;
  bundleFile: string;
  stream?: boolean;
}
