import $ from "dax";

const version = await resolveVersion();
const platform = currentPlatform();
const outputDir = pathJoin(Deno.cwd(), "dist");
const binaryPath = pathJoin(
  Deno.cwd(),
  "target",
  "release",
  Deno.build.os === "windows" ? "bastest.exe" : "bastest",
);

await Deno.remove(outputDir, { recursive: true }).catch(() => {});
await $`deno task build`;
await $`cargo build --release -p bastest`;
await $`deno run --allow-read --allow-write scripts/prepare-platform-package.ts ${platform} ${binaryPath} ${version}`;
await $`deno run --allow-read --allow-write scripts/prepare-npm-package.ts ${version}`;

console.log(`prepared npm packages in ${outputDir}`);

async function resolveVersion(): Promise<string> {
  const explicit = Deno.args[0]?.replace(/^v/, "");
  if (explicit) {
    return explicit;
  }

  const packageJson = JSON.parse(
    await Deno.readTextFile(
      pathJoin(Deno.cwd(), "packages", "bastest", "package.json"),
    ),
  );

  return String(packageJson.version).replace(/^v/, "");
}

function currentPlatform(): string {
  const { os, arch } = Deno.build;

  if (os === "windows" && arch === "x86_64") {
    return "win32-x64";
  }

  if (os === "windows" && arch === "aarch64") {
    return "win32-arm64";
  }

  if (os === "darwin" && arch === "aarch64") {
    return "darwin-arm64";
  }

  if (os === "linux" && arch === "x86_64") {
    return "linux-x64-musl";
  }

  if (os === "linux" && arch === "aarch64") {
    return "linux-arm64-musl";
  }

  console.error(`unsupported platform: ${os}-${arch}`);
  Deno.exit(2);
}

function pathJoin(...parts: string[]): string {
  return parts.join(Deno.build.os === "windows" ? "\\" : "/");
}
