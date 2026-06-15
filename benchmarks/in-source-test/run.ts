import { cases } from "./cases.ts";
import { prepareFixture } from "./fixtures.ts";
import { runOrThrow } from "./runner.ts";

prepareFixture();

for (const benchCase of cases) {
  Deno.bench(benchCase.name, (bench) => {
    runOrThrow(benchCase, bench);
  });
}
