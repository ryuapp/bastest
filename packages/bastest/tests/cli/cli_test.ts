import { spawnSync } from "node:child_process";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { assert, test } from "bastest";

const packageRoot = process.cwd();
const cli = path.resolve(
  packageRoot,
  "..",
  "..",
  "target",
  "debug",
  process.platform === "win32" ? "bastest.exe" : "bastest",
);
const fixtures = path.join(packageRoot, "tests", "fixtures");

test("CLI runs explicit test files", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const file = path.join(dir, "sample_test.ts");
    await writeFile(
      file,
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("generated pass", () => assert(1 + 1 === 2));',
      ].join("\n"),
      "utf8",
    );

    const result = runCli(file);
    assert(result.status === 0, result.stderr);
    assert(/generated pass \.\.\. ok/.test(result.stdout));
    assert(/ok \| 1 passed \| 0 failed \| 0 ignored/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI returns non-zero when a test fails", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const file = path.join(dir, "sample_test.ts");
    await writeFile(
      file,
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("generated fail", () => assert(1 === 2));',
      ].join("\n"),
      "utf8",
    );

    const result = runCli(file);
    assert(result.status === 1);
    assert(/generated fail \.\.\. fail/.test(result.stdout));
    assert(/Expected/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI discovers in-source tests by default", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "src"), { recursive: true });
    await writeFile(path.join(dir, "bastest.jsonc"), "{}\n", "utf8");
    await writeFile(
      path.join(dir, "src", "source.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        "export const value = 1;",
        `if (import.meta${".test"}) {`,
        '  test("in-source pass", () => assert(value === 1));',
        "}",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 0, result.stderr);
    assert(/in-source pass \.\.\. ok/.test(result.stdout));
    assert(/ok \| 1 passed \| 0 failed \| 0 ignored/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI uses bastest.jsonc directory as project root", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "src"), { recursive: true });
    await mkdir(path.join(dir, "nested"), { recursive: true });
    await writeFile(path.join(dir, "bastest.jsonc"), "{}\n", "utf8");
    await writeFile(
      path.join(dir, "src", "source.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        `if (import.meta${".test"}) {`,
        '  test("project root source", () => assert(true));',
        "}",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(path.join(dir, "nested"));
    assert(result.status === 0, result.stderr);
    assert(/project root source \.\.\. ok/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI fails when bastest.jsonc is not found", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "src"), { recursive: true });
    await writeFile(
      path.join(dir, "src", "source.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        `if (import.meta${".test"}) {`,
        '  test("fallback source", () => assert(true));',
        "}",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 2);
    assert(
      result.stderr.includes(
        `could not find \`bastest.jsonc\` in \`${dir}\` or any parent directory`,
      ),
    );
    assert(!/fallback source \.\.\. ok/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI can disable in-source test discovery", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "src"), { recursive: true });
    await writeFile(
      path.join(dir, "bastest.jsonc"),
      '{\n  "inSourceTest": false\n}\n',
      "utf8",
    );
    await writeFile(
      path.join(dir, "src", "source.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        `if (import.meta${".test"}) {`,
        '  test("should not be discovered", () => assert(false));',
        "}",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 0, result.stderr);
    assert(result.stdout.includes("No test files found."));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI excludes configured paths from discovery", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "src", "ignored"), { recursive: true });
    await writeFile(
      path.join(dir, "bastest.jsonc"),
      '{\n  "exclude": ["src/ignored"]\n}\n',
      "utf8",
    );
    await writeFile(
      path.join(dir, "src", "ignored", "source.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        `if (import.meta${".test"}) {`,
        '  test("excluded source", () => assert(false));',
        "}",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 0, result.stderr);
    assert(result.stdout.includes("No test files found."));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI excludes .git and node_modules by default", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, ".git", "hooks"), { recursive: true });
    await mkdir(path.join(dir, "node_modules", "pkg"), { recursive: true });
    await mkdir(path.join(dir, "dist"), { recursive: true });
    await writeFile(path.join(dir, "bastest.jsonc"), "{}\n", "utf8");
    await writeFile(
      path.join(dir, ".git", "hooks", "bad_test.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("git test", () => assert(false));',
      ].join("\n"),
      "utf8",
    );
    await writeFile(
      path.join(dir, "node_modules", "pkg", "bad_test.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("node_modules test", () => assert(false));',
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 0, result.stderr);
    assert(result.stdout.includes("No test files found."));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI config exclude overrides default excludes", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "node_modules", "pkg"), { recursive: true });
    await writeFile(
      path.join(dir, "bastest.jsonc"),
      '{\n  "exclude": [".git"]\n}\n',
      "utf8",
    );
    await writeFile(
      path.join(dir, "node_modules", "pkg", "good_test.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("node_modules test", () => assert(true));',
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 0, result.stderr);
    assert(/node_modules test \.\.\. ok/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI uses minimal output when agent config is enabled", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const file = path.join(dir, "sample_test.ts");
    await writeFile(
      path.join(dir, "bastest.jsonc"),
      '{\n  // JSONC is supported.\n  "agent": true,\n}\n',
      "utf8",
    );
    await writeFile(
      file,
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("generated fail", () => {',
        '  const user = { name: "alpha" };',
        '  const expected = "beta";',
        "  assert(user.name === expected);",
        "});",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 1);
    assert(!result.stdout.includes("[bastest.ai]"));
    assert(result.stdout.includes("generated fail"));
    assert(result.stdout.includes("assert(user.name === expected)"));
    assert(result.stdout.includes('"alpha"'));
    assert(result.stdout.includes('"beta"'));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI enables minimal output from agent env", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const file = path.join(dir, "sample_test.ts");
    await writeFile(path.join(dir, "bastest.jsonc"), "{}\n", "utf8");
    await writeFile(
      file,
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("generated fail", () => assert(false));',
      ].join("\n"),
      "utf8",
    );

    const result = runCliWithEnv(dir, { AI_AGENT: "1" });
    assert(result.status === 1);
    assert(!result.stdout.includes("[bastest.ai]"));
    assert(result.stdout.includes("generated fail"));
    assert(!result.stdout.includes("generated pass"));
    assert(result.stdout.includes("Report Dir: "));
    assert(result.stdout.includes(".bastest/reports/latest"));
    assert(
      result.stdout.indexOf("fail |") < result.stdout.indexOf("generated fail"),
    );
    assert(
      result.stdout.indexOf("Report Dir: ") <
        result.stdout.indexOf("generated fail"),
    );
    await access(
      path.join(dir, ".bastest", "reports", "latest", "summary.json"),
    );
    const failures = JSON.parse(
      await readFile(
        path.join(dir, ".bastest", "reports", "latest", "failures.json"),
        "utf8",
      ),
    );
    assert(failures.summary.failed === 1);
    assert(failures.failures.length === 1);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI enables minimal output from --agent", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const file = path.join(dir, "sample_test.ts");
    await writeFile(path.join(dir, "bastest.jsonc"), "{}\n", "utf8");
    await writeFile(
      file,
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("generated pass", () => assert(true));',
        'test("generated fail", () => assert(false));',
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir, "--agent");
    assert(result.status === 1);
    assert(!result.stdout.includes("[bastest.ai]"));
    assert(result.stdout.includes("generated fail"));
    assert(!result.stdout.includes("generated pass"));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI does not print report path for successful agent runs", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const file = path.join(dir, "sample_test.ts");
    await writeFile(path.join(dir, "bastest.jsonc"), "{}\n", "utf8");
    await writeFile(
      file,
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        'test("generated pass", () => assert(true));',
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir, "--agent");
    assert(result.status === 0);
    assert(!result.stdout.includes("Report Dir: "));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI runs typecheck before tests when enabled", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await writeFile(
      path.join(dir, "bastest.jsonc"),
      '{\n  "typecheck": {\n    "enabled": true,\n    "checker": "tsgo",\n  },\n}\n',
      "utf8",
    );
    await writeFile(
      path.join(dir, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: { strict: true },
        include: ["sample_test.ts"],
      }),
      "utf8",
    );
    await writeFile(
      path.join(dir, "sample_test.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        "function accepts(value: string | undefined) {",
        "  assert<string>(value);",
        "}",
        'test("should not run", () => assert(accepts("1") === undefined));',
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status !== 0);
    assert(/typecheck tsgo/.test(result.stdout));
    assert(/TypeCheckError:/.test(result.stdout + result.stderr));
    assert(/assert<string>\(value\)/.test(result.stdout + result.stderr));
    assert(/string \| undefined/.test(result.stdout + result.stderr));
    assert(!/PASS should not run/.test(result.stdout));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI report output matches pass skip fail fixture", async () => {
  const file = path.join(fixtures, "report", "basic_test.ts");
  const expected = await readFixture("report", "basic.expected.txt");

  const result = runCli(file);
  assert(result.status === 1);
  assert(normalizeLog(result.stdout) === expected);
  assert(result.stderr === "");
});

test("CLI typecheck output matches fixture", async () => {
  const cwd = path.join(fixtures, "typecheck");
  const expected = await readFixture("typecheck", "typecheck.expected.txt");

  const result = runCliIn(cwd);
  assert(result.status === 1);
  assert(normalizeLog(result.stdout + result.stderr) === expected);
});

function runCli(
  ...args: string[]
): { status: number | null; stdout: string; stderr: string } {
  return runCliIn(packageRoot, ...args);
}

function runCliWithEnv(
  cwd: string,
  env: Record<string, string>,
  ...args: string[]
): { status: number | null; stdout: string; stderr: string } {
  return runCliIn(cwd, ...args, env);
}

function runCliIn(
  cwd: string,
  ...argsAndEnv: Array<string | Record<string, string>>
): { status: number | null; stdout: string; stderr: string } {
  const env = typeof argsAndEnv.at(-1) === "object"
    ? (argsAndEnv.pop() as Record<string, string>)
    : {};
  const args = argsAndEnv as string[];
  const processEnv = { ...process.env };
  delete processEnv.AI_AGENT;
  delete processEnv.CODEX_THREAD_ID;
  delete processEnv.CLAUDECODE;

  const result = spawnSync(cli, args, {
    cwd,
    encoding: "utf8",
    env: {
      ...processEnv,
      NO_COLOR: "1",
      BASTEST_RUNTIME_PATH: process.execPath,
      BASTEST_PACKAGE_ROOT: packageRoot,
      ...env,
    },
  });

  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

async function readFixture(...parts: string[]): Promise<string> {
  return normalizeNewlines(
    await readFile(path.join(fixtures, ...parts), "utf8"),
  ).trimEnd();
}

function normalizeLog(value: string): string {
  return normalizeNewlines(value)
    .replaceAll("\\", "/")
    .replace(/(?<![\w.])\d+(?:\.\d+)?ms/g, "<duration>")
    .replace(/(?<![\w.])\d+(?:\.\d+)?s/g, "<duration>")
    .replace(/^[ \t]+at .*$/gm, "    <stack>")
    .trimEnd();
}

function normalizeNewlines(value: string): string {
  return value.replace(/\r\n/g, "\n");
}
