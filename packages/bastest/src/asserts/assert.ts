import { AssertionError } from "./error.ts";

export interface AssertMetadata {
  expression?: string;
  captures?: Array<
    [source: string, start: number, end: number, capture: () => unknown]
  >;
}

export function assert(
  value: unknown,
  message?: string | AssertMetadata,
  metadata?: AssertMetadata,
): asserts value {
  if (!value) {
    const details = typeof message === "object" ? message : metadata;
    throw new AssertionError({
      message: typeof message === "string"
        ? message
        : "Expected value to be truthy",
      actual: value,
      expected: true,
      operator: "assert",
      expression: details?.expression,
      captures: details?.captures?.map(([source, start, end, capture]) => ({
        source,
        start,
        end,
        value: captureValue(capture),
      })),
    });
  }
}

function captureValue(capture: () => unknown): unknown {
  try {
    const value = capture();
    if (typeof value === "function") {
      return `[Function ${value.name || "anonymous"}]`;
    }
    if (value === undefined) {
      return "undefined";
    }
    return value;
  } catch (error) {
    return `<failed to capture: ${
      error instanceof Error ? error.message : String(error)
    }>`;
  }
}
