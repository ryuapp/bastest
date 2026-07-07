import { assertSnapshot, test } from "bastest";

test("records value", () => {
  assertSnapshot({ name: "bastest", marker: "import.meta.test", ok: true });
});
test("duplicate name", () => {
  assertSnapshot({ duplicate: 1 });
});
test("duplicate name", () => {
  assertSnapshot({ duplicate: 2 });
});
