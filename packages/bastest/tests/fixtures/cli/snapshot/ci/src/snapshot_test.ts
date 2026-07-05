import { assertSnapshot, test } from "bastest";

test("records value", () => {
  assertSnapshot({ name: "bastest", ok: true });
});
