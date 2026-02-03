import "dotenv/config";
import { compute } from "computesdk";
import { runPrompt, waitForHealth } from "@sandbox-agent/example-shared";

export async function setupComputeSDKSandboxAgent() {
  const envs: Record<string, string> = {};
  if (process.env.ANTHROPIC_API_KEY) envs.ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY;
  if (process.env.OPENAI_API_KEY) envs.OPENAI_API_KEY = process.env.OPENAI_API_KEY;

  console.log("Creating ComputeSDK sandbox...");
  const sandbox = await compute.sandbox.create({ envs });

  const run = async (cmd: string) => {
    const result = await sandbox.runCommand(cmd);
    if (result.exitCode !== 0) throw new Error(`Command failed: ${cmd}\n${result.stderr}`);
    return result;
  };

  console.log("Installing sandbox-agent...");
  // Install to ~/.local/bin for sandboxes without sudo
  await run("mkdir -p ~/.local/bin");
  await run("curl -fsSL https://releases.rivet.dev/sandbox-agent/latest/install.sh | BIN_DIR=~/.local/bin sh");

  console.log("Installing agents...");
  await run("~/.local/bin/sandbox-agent install-agent claude");
  await run("~/.local/bin/sandbox-agent install-agent codex");

  console.log("Starting server...");
  await sandbox.runCommand("~/.local/bin/sandbox-agent server --no-token --host 0.0.0.0 --port 3000", {
    background: true,
    env: envs,
  });

  const baseUrl = await sandbox.getUrl({ port: 3000 });

  console.log("Waiting for server...");
  await waitForHealth({ baseUrl });

  const cleanup = async () => {
    await sandbox.destroy();
  };

  return { baseUrl, token: undefined, cleanup };
}

// Run interactively when executed directly
const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const { baseUrl, cleanup } = await setupComputeSDKSandboxAgent();

  let isCleaningUp = false;
  const exitCleanup = async () => {
    if (isCleaningUp) return;
    isCleaningUp = true;
    console.log("\nDestroying sandbox...");
    try {
      await cleanup();
      console.log("Sandbox destroyed.");
    } catch (err) {
      console.error("Failed to destroy sandbox:", err);
    }
    process.exit(0);
  };

  process.on("SIGINT", () => {
    void exitCleanup();
  });
  process.on("SIGTERM", () => {
    void exitCleanup();
  });

  try {
    await runPrompt(baseUrl);
  } catch (err: unknown) {
    // Ignore AbortError from Ctrl+C
    if (err instanceof Error && err.name !== "AbortError") {
      console.error("Error:", err.message);
    }
  }
  await exitCleanup();
}
