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

export interface Registry {
  api: { test: TestApi };
  runAll(
    options?: { filter?: RegExp; onTest?: (result: TestResult) => void },
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
        const result = await runTest(entry);
        results.push(result);
        options?.onTest?.(result);
      }
      return results;
    },
  };
}

async function runTest(entry: RegisteredTest): Promise<TestResult> {
  if (entry.ignore) {
    return {
      name: entry.name,
      status: "ignored",
      durationMs: 0,
      steps: [],
    };
  }

  const steps: StepResult[] = [];
  const started = performance.now();
  globalThis.__bastest_snapshot = {
    cwd: globalThis.__bastest_current_cwd,
    file: globalThis.__bastest_current_file ?? "",
    testName: entry.name,
    testNameOccurrence: entry.nameOccurrence,
    index: 0,
  };
  try {
    await entry.fn(createContext(entry.name, steps));
    return {
      name: entry.name,
      status: "passed",
      durationMs: elapsed(started),
      steps,
    };
  } catch (error) {
    return {
      name: entry.name,
      status: "failed",
      durationMs: elapsed(started),
      error: serializeError(error),
      steps,
    };
  } finally {
    globalThis.__bastest_snapshot = undefined;
  }
}

function createContext(name: string, sink: StepResult[]): TestContext {
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

      const started = performance.now();
      try {
        await options.fn(createContext(options.name, childSteps));
        sink.push({
          name: options.name,
          status: "passed",
          durationMs: elapsed(started),
          steps: childSteps,
        });
      } catch (error) {
        const result: StepResult = {
          name: options.name,
          status: "failed",
          durationMs: elapsed(started),
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

function elapsed(started: number): number {
  return Math.round((performance.now() - started) * 100) / 100;
}
