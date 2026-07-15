export type TestFunction = (t: TestContext) => unknown | Promise<unknown>;

export interface TestContext {
  readonly name: string;
  step(name: string, fn: TestFunction): Promise<void>;
  step(options: StepOptions): Promise<void>;
}

export interface TestOptions {
  name: string;
  fn: TestFunction;
  ignore?: boolean;
  only?: boolean;
}

export interface StepOptions {
  name: string;
  fn: TestFunction;
  ignore?: boolean;
}

export interface TestApi {
  (name: string, fn: TestFunction): void;
  (options: TestOptions): void;
  ignore(name: string, fn: TestFunction): void;
  only(name: string, fn: TestFunction): void;
}

declare global {
  // The node runner installs this before importing test files.
  var __bastest_api: { test: TestApi } | undefined;
  var __bastest_current_cwd: string | undefined;
  var __bastest_current_file: string | undefined;

  interface ImportMeta {
    readonly test: boolean;
  }
}

const api = globalThis.__bastest_api;

if (!api) {
  throw new Error(
    "bastest test files must be executed by the bastest CLI. Run `bastest` instead of importing `bastest` directly.",
  );
}

export const test = api.test;
export { assert } from "./asserts/assert.ts";
export { assertType } from "./asserts/assert_type.ts";
export { assertSnapshot } from "./asserts/snapshot.ts";
