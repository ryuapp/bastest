import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const platform = platformPackageName();
const binName = process.platform === "win32" ? "bastest.exe" : "bastest";
const binary = path.join(root, "target", "release", binName);
const packageDir = path.join(root, "node_modules", "@bastest", platform);

try {
  await Deno.stat(binary);
} catch {
  console.error(`missing release binary: ${binary}`);
  console.error("run `deno task build:rust` first");
  Deno.exit(1);
}

await Deno.remove(packageDir, { recursive: true }).catch(() => {});
await Deno.mkdir(packageDir, { recursive: true });
await Deno.copyFile(binary, path.join(packageDir, binName));

await Deno.writeTextFile(
  path.join(packageDir, "package.json"),
  JSON.stringify(
    {
      name: `@bastest/${platform}`,
      version: "0.0.0",
      type: "module",
      files: [binName],
    },
    null,
    2,
  ) + "\n",
);

function platformPackageName(): string {
  const key = `${process.platform}-${process.arch}`;
  switch (key) {
    case "linux-x64":
      return "linux-x64-musl";
    case "linux-arm64":
      return "linux-arm64-musl";
    case "darwin-arm64":
      return "darwin-arm64";
    case "win32-x64":
      return "win32-x64";
    case "win32-arm64":
      return "win32-arm64";
    default:
      console.error(`unsupported platform: ${key}`);
      Deno.exit(1);
  }
}
