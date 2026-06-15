import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const PLATFORM_MAP = {
  "linux-x64": "linux-x64-musl",
  "linux-arm64": "linux-arm64-musl",
  "darwin-arm64": "darwin-arm64",
  "win32-x64": "win32-x64",
  "win32-arm64": "win32-arm64",
};

const platformKey = `${process.platform}-${process.arch}`;
const pkgName = PLATFORM_MAP[platformKey];

if (!pkgName) {
  console.error(`Unsupported platform: ${platformKey}`);
  process.exit(1);
}

const binName = process.platform === "win32" ? "bastest.exe" : "bastest";
const fullPkgName = `@bastest/${pkgName}`;
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

let binPath;
try {
  const pkgJsonPath = fileURLToPath(
    import.meta.resolve(`${fullPkgName}/package.json`),
  );
  binPath = join(dirname(pkgJsonPath), binName);
} catch (error) {
  console.error(`Failed to find ${fullPkgName}. Please reinstall bastest.`);
  console.error(
    "Error:",
    error instanceof Error ? error.message : String(error),
  );
  process.exit(1);
}

const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: "inherit",
  env: {
    ...process.env,
    BASTEST_RUNTIME_PATH: process.argv0,
    BASTEST_PACKAGE_ROOT: packageRoot,
  },
});
process.exit(result.status ?? 1);
