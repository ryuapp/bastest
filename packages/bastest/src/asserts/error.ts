export class AssertionError extends Error {
  readonly actual?: unknown;
  readonly expected?: unknown;
  readonly operator?: string;
  readonly expression?: string;
  readonly captures?: AssertionCapture[];

  constructor(
    messageOrOptions: string | AssertionErrorOptions = "Assertion failed",
  ) {
    const options = typeof messageOrOptions === "string"
      ? { message: messageOrOptions }
      : messageOrOptions;
    super(options.message ?? "Assertion failed");
    this.name = "AssertionError";
    this.actual = options.actual;
    this.expected = options.expected;
    this.operator = options.operator;
    this.expression = options.expression;
    this.captures = options.captures;
  }
}

export interface AssertionCapture {
  source: string;
  start: number;
  end: number;
  value: unknown;
}

export interface AssertionErrorOptions {
  message?: string;
  actual?: unknown;
  expected?: unknown;
  operator?: string;
  expression?: string;
  captures?: AssertionCapture[];
}
