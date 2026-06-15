export interface RunnerOptions {
  worker: boolean;
  file?: string;
  bundleFile?: string;
  filter?: string;
}

export interface RunFileOptions {
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
