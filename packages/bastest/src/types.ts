import type { TestFunction } from "./mod.ts";

export type TestStatus = "passed" | "failed" | "ignored";

export interface RegisteredTest {
  id: number;
  name: string;
  nameOccurrence: number;
  fn: TestFunction;
  ignore: boolean;
  only: boolean;
}

export interface StepResult {
  name: string;
  status: TestStatus;
  durationMs: number;
  error?: SerializedError;
  steps: StepResult[];
}

export interface TestResult {
  name: string;
  status: TestStatus;
  durationMs: number;
  error?: SerializedError;
  steps: StepResult[];
}

export interface FileResult {
  file: string;
  durationMs: number;
  tests: TestResult[];
  loadError?: SerializedError;
}

export interface SerializedError {
  name: string;
  message: string;
  stack?: string;
  actual?: unknown;
  expected?: unknown;
  operator?: string;
  expression?: string;
  captures?: Array<
    { source: string; start: number; end: number; value: unknown }
  >;
}
