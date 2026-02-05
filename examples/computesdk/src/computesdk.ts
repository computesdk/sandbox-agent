import "dotenv/config";
import { compute } from "computesdk";
import { runPrompt } from "@sandbox-agent/example-shared";

export async function setupComputeSDKSandboxAgent() {
  const envs: Record<string, string> = {};
  if (process.env.ANTHROPIC_API_KEY) envs.ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY;
  if (process.env.OPENAI_API_KEY) envs.OPENAI_API_KEY = process.env.OPENAI_API_KEY;

  console.log("Creating ComputeSDK sandbox...");
  const sandbox = await compute.sandbox.create({ envs });

  console.log("Starting sandbox-agent server...");
  const server = await sandbox.server.start({
    slug: "sandbox-agent",
    // Install commands run first (blocking)
    install:
      "mkdir -p ~/.local/bin && " +
      "curl -fsSL https://releases.rivet.dev/sandbox-agent/latest/install.sh | BIN_DIR=~/.local/bin sh && " +
      "~/.local/bin/sandbox-agent install-agent claude && " +
      "~/.local/bin/sandbox-agent install-agent codex",
    // Start command runs after install completes
    start: "~/.local/bin/sandbox-agent server --no-token --host 0.0.0.0 --port 3000",
    port: 3000,
    environment: envs,
    // Built-in health check - status becomes 'ready' only after this passes
    health_check: {
      path: "/v1/health",
      interval_ms: 2000,
      timeout_ms: 5000,
      delay_ms: 3000,
    },
    restart_policy: "on-failure",
    max_restarts: 3,
  });

  // Wait for server to be ready or running with URL
  console.log("Waiting for server to be ready...");
  let currentServer = server;
  let baseUrl: string | undefined;
  const maxAttempts = 60;
  let attempts = 0;

  while (attempts < maxAttempts) {
    await new Promise((r) => setTimeout(r, 1000));
    currentServer = await sandbox.server.retrieve("sandbox-agent");
    console.log(`Server status: ${currentServer.status}, url: ${currentServer.url || "not available"}`);

    // If we have a URL and status is running or ready, try to use it
    if (currentServer.url && (currentServer.status === "running" || currentServer.status === "ready")) {
      baseUrl = currentServer.url;
      // Try a manual health check
      try {
        const healthResp = await fetch(`${baseUrl}/v1/health`, { signal: AbortSignal.timeout(5000) });
        if (healthResp.ok) {
          console.log("Health check passed!");
          break;
        }
      } catch {
        // Health check failed, keep waiting
      }
    }

    // Check for failure states
    if (currentServer.status === "failed" || currentServer.status === "stopped") {
      const logs = await sandbox.server.logs("sandbox-agent");
      console.error("Server logs:", logs.logs);
      throw new Error(`Server failed to start: ${currentServer.status}`);
    }

    attempts++;
  }

  if (!baseUrl) {
    // Fallback: try to get URL directly from sandbox
    baseUrl = await sandbox.getUrl({ port: 3000 });
    console.log(`Using fallback URL: ${baseUrl}`);
  }

  if (!baseUrl) {
    throw new Error("Could not obtain server URL");
  }

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
