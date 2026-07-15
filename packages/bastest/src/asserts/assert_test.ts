import { assert, test } from "bastest";

test("assert validates truthy values", () => {
  assert(true);
  const defaultError = catchError(() => assert(false));
  assert(defaultError?.name === "AssertionError");
  assert(defaultError?.message === "Expected value to be truthy");

  const customError = catchError(() => assert(0, "custom truthy message"));
  assert(customError?.name === "AssertionError");
  assert(customError?.message === "custom truthy message");
});

test("assert reports captured values", () => {
  const user = { name: "alice" };
  const expected = "bob";
  const error = catchError(() => assert(user.name === expected));
  assert(error?.name === "AssertionError");
  assert(error.expression === "user.name === expected");
  assert(
    JSON.stringify(error.captures) ===
      JSON.stringify([
        { source: "user", start: 0, end: 4, value: user },
        { source: "user.name", start: 0, end: 9, value: "alice" },
        { source: "expected", start: 14, end: 22, value: expected },
      ]),
  );
});

test("assert narrows values", () => {
  const value: string | undefined = "bastest";
  assert(value);
  const narrowed: string = value;
  assert(narrowed === "bastest");
});

function catchError(fn: () => void): Record<string, unknown> | undefined {
  try {
    fn();
    return undefined;
  } catch (error) {
    assert(error instanceof Error);
    return error as unknown as Record<string, unknown>;
  }
}
