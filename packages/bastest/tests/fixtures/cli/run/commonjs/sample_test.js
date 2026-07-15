import { JSDOM } from "jsdom";
import * as unrun from "unrun";
import { assert, test } from "bastest";

test("loads jsdom", () => {
  assert(new JSDOM("<p>ok</p>").window.document.body.textContent === "ok");
  assert(typeof unrun === "object");
});
