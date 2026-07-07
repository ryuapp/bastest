import type {
  RegisteredTest,
  SerializedError,
  StepResult,
  TestResult,
} from "./types.ts";
import type {
  StepOptions,
  TestApi,
  TestContext,
  TestFunction,
  TestOptions,
} from "./mod.ts";
import type { RuntimePlugin } from "./runtime/plugin.ts";
import type { SnapshotContext } from "./snapshot/context.ts";

const testContextKey = Symbol.for("bastest.test.context");

export interface TestExecutionContext {
  runtime: RuntimePlugin;
  snapshot: SnapshotContext;
}

interface TestContextGlobal {
  [testContextKey]?: TestExecutionContext;
}

export function getTestExecutionContext(): TestExecutionContext | undefined {
  return (globalThis as TestContextGlobal)[testContextKey];
}

export interface Registry {
  api: { test: TestApi };
  runAll(
    options: {
      runtime: RuntimePlugin;
      filter?: RegExp;
      onTest?: (result: TestResult) => void;
    },
  ): Promise<TestResult[]>;
}

export function createRegistry(): Registry {
  const tests: RegisteredTest[] = [];
  const nameOccurrences = new Map<string, number>();
  let nextId = 0;

  const register = (options: TestOptions) => {
    const nameOccurrence = (nameOccurrences.get(options.name) ?? 0) + 1;
    nameOccurrences.set(options.name, nameOccurrence);
    tests.push({
      id: nextId++,
      name: options.name,
      nameOccurrence,
      fn: options.fn,
      ignore: options.ignore === true,
      only: options.only === true,
    });
  };

  const test = ((nameOrOptions: string | TestOptions, fn?: TestFunction) => {
    if (typeof nameOrOptions === "string") {
      if (!fn) {
        throw new TypeError("test(name, fn) requires a function");
      }
      register({ name: nameOrOptions, fn });
      return;
    }
    register(nameOrOptions);
  }) as TestApi;

  test.ignore = (name, fn) => register({ name, fn, ignore: true });
  test.only = (name, fn) => register({ name, fn, only: true });

  return {
    api: { test },
    async runAll(options) {
      const hasOnly = tests.some((entry) => entry.only);
      const selected = tests.filter((entry) => {
        if (hasOnly && !entry.only) {
          return false;
        }
        if (options?.filter && !options.filter.test(entry.name)) {
          return false;
        }
        return true;
      });

      const results: TestResult[] = [];
      for (const entry of selected) {
        const result = await runTest(entry, options.runtime);
        results.push(result);
        options?.onTest?.(result);
      }
      return results;
    },
  };
}

async function runTest(
  entry: RegisteredTest,
  runtime: RuntimePlugin,
): Promise<TestResult> {
  if (entry.ignore) {
    return {
      name: entry.name,
      status: "ignored",
      durationMs: 0,
      steps: [],
    };
  }

  const steps: StepResult[] = [];
  const started = runtime.now();
  const snapshotContext: SnapshotContext = {
    cwd: globalThis.__bastest_current_cwd,
    file: globalThis.__bastest_current_file ?? "",
    testName: entry.name,
    testNameOccurrence: entry.nameOccurrence,
    index: 0,
  };
  (globalThis as TestContextGlobal)[testContextKey] = {
    runtime,
    snapshot: snapshotContext,
  };
  try {
    await entry.fn(createContext(entry.name, steps, runtime));
    return {
      name: entry.name,
      status: "passed",
      durationMs: elapsed(runtime, started),
      steps,
    };
  } catch (error) {
    return {
      name: entry.name,
      status: "failed",
      durationMs: elapsed(runtime, started),
      error: serializeError(error),
      steps,
    };
  } finally {
    delete (globalThis as TestContextGlobal)[testContextKey];
  }
}

function createContext(
  name: string,
  sink: StepResult[],
  runtime: RuntimePlugin,
): TestContext {
  return {
    name,
    async step(nameOrOptions: string | StepOptions, fn?: TestFunction) {
      let options: StepOptions;
      if (typeof nameOrOptions === "string") {
        if (!fn) {
          throw new TypeError("t.step(name, fn) requires a function");
        }
        options = { name: nameOrOptions, fn };
      } else {
        options = nameOrOptions;
      }

      const childSteps: StepResult[] = [];
      if (options.ignore) {
        sink.push({
          name: options.name,
          status: "ignored",
          durationMs: 0,
          steps: [],
        });
        return;
      }

      const started = runtime.now();
      try {
        await options.fn(createContext(options.name, childSteps, runtime));
        sink.push({
          name: options.name,
          status: "passed",
          durationMs: elapsed(runtime, started),
          steps: childSteps,
        });
      } catch (error) {
        const result: StepResult = {
          name: options.name,
          status: "failed",
          durationMs: elapsed(runtime, started),
          error: serializeError(error),
          steps: childSteps,
        };
        sink.push(result);
        throw error;
      }
    },
  };
}

export function serializeError(error: unknown): SerializedError {
  if (error instanceof Error) {
    const assertion = error as Error & {
      actual?: unknown;
      expected?: unknown;
      operator?: string;
      expression?: string;
      captures?: Array<
        { source: string; start: number; end: number; value: unknown }
      >;
    };
    return {
      name: error.name,
      message: error.message,
      stack: error.stack,
      actual: assertion.actual,
      expected: assertion.expected,
      operator: assertion.operator,
      expression: assertion.expression,
      captures: assertion.captures,
    };
  }
  return {
    name: "Error",
    message: String(error),
  };
}

function elapsed(runtime: RuntimePlugin, started: number): number {
  return Math.round((runtime.now() - started) * 100) / 100;
}
