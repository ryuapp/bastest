import { assert, test } from "bastest";

test("test context exposes the current test name", (t) => {
  assert(t.name === "test context exposes the current test name");
});

test("steps can be nested", async (t) => {
  const events: string[] = [];

  await t.step("outer", async (outer) => {
    events.push(outer.name);
    await outer.step("inner", (inner) => {
      events.push(inner.name);
    });
  });

  assert(JSON.stringify(events) === JSON.stringify(["outer", "inner"]));
});

test.ignore("ignored tests are reported but not executed", () => {
  throw new Error("ignored test executed");
});
