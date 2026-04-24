// @ts-nocheck
import fs from 'node:fs';

import { loadConfig, loadState, printChecks, resolveNodeAgentBinary, resolveRunConfig, saveConfig, saveState, ensureInferDirs, checkDoctor, verifyRemoteRegistration } from '../lib/app.js';
import { fileExists } from '../lib/fs.js';
import { pullModel, ollamaTags } from '../lib/http.js';
import { spawnDetached, spawnForeground, wait, waitForHealth, isPidAlive } from '../lib/process.js';

export async function runCommand(flags: Record<string, any>, positionals: string[]) {
  const model = positionals[0];
  if (!model) {
    throw new Error('Usage: infer run <model> [--gateway ... --token ...]');
  }

  const existing = (await loadConfig()) || {};
  const previousState = await loadState();
  if (previousState?.pid && isPidAlive(previousState.pid)) {
    throw new Error(`A node-agent is already running with PID ${previousState.pid}. Stop it first with 'infer stop'.`);
  }

  const config = resolveRunConfig(model, flags, existing);
  const paths = await ensureInferDirs();
  const checks = await checkDoctor({ ...config, agentBin: flags['agent-bin'] });
  printChecks(checks);
  if (checks.some((check) => !check.ok && !String(check.label).startsWith('Model missing in Ollama'))) {
    throw new Error('Doctor checks failed. Fix the failing checks and retry.');
  }

  const tags = await ollamaTags(config.ollamaUrl);
  const models = Array.isArray(tags.body?.models) ? tags.body.models.map((entry: any) => entry.name) : [];
  if (!models.includes(model)) {
    if (flags['no-pull'] === true) {
      throw new Error(`Model ${model} is missing in Ollama and --no-pull was provided`);
    }
    console.log(`Pulling model ${model} from Ollama...`);
    await pullModel(config.ollamaUrl, model);
    console.log(`✓ Model pulled: ${model}`);
  }

  const binaryPath = await resolveNodeAgentBinary(flags);
  if (!(await fileExists(binaryPath))) {
    throw new Error(`node-agent binary not found at ${binaryPath}`);
  }

  await fs.promises.rm(paths.logFile, { force: true });
  const env = {
    NODE_NAME: config.nodeName,
    NODE_HOST: config.host,
    NODE_PORT: String(config.ollamaPort),
    AGENT_PORT: String(config.agentPort),
    COORDINATOR_URL: config.gatewayUrl,
    INFER_INTERNAL_KEY: config.internalKey,
    NODE_MODEL: config.model,
  };

  await saveConfig({
    model: config.model,
    gatewayUrl: config.gatewayUrl,
    internalKey: config.internalKey,
    ollamaUrl: config.ollamaUrl,
    ollamaPort: config.ollamaPort,
    agentPort: config.agentPort,
    nodeName: config.nodeName,
    host: config.host,
    agentBin: binaryPath,
    updatedAt: new Date().toISOString(),
  });

  if (config.detach) {
    const child = spawnDetached(binaryPath, [], env, paths.logFile);
    await saveState({
      pid: child.pid,
      status: 'starting',
      agentPort: config.agentPort,
      binaryPath,
      logPath: paths.logFile,
      startedAt: new Date().toISOString(),
      mode: 'detached',
    });

    const health = await waitForHealth(config.agentPort, 15000);
    let remoteNode = null;
    for (let i = 0; i < 10; i++) {
      remoteNode = await verifyRemoteRegistration(config.gatewayUrl, config.internalKey, config.nodeName, config.model).catch(() => null);
      if (remoteNode) break;
      await wait(500);
    }

    await saveState({
      pid: child.pid,
      status: 'running',
      agentPort: config.agentPort,
      binaryPath,
      logPath: paths.logFile,
      startedAt: new Date().toISOString(),
      lastHealthCheckAt: new Date().toISOString(),
      mode: 'detached',
    });

    console.log(`✓ node-agent started in detached mode (PID ${child.pid})`);
    console.log(`✓ Local health: ${health.status} registered=${health.registered}`);
    console.log(remoteNode ? '✓ Remote registration verified' : '• Remote registration not yet verified');
    return;
  }

  const { child, log } = spawnForeground(binaryPath, [], env, paths.logFile);
  await saveState({
    pid: child.pid,
    status: 'running',
    agentPort: config.agentPort,
    binaryPath,
    logPath: paths.logFile,
    startedAt: new Date().toISOString(),
    lastHealthCheckAt: new Date().toISOString(),
    mode: 'foreground',
  });

  const health = await waitForHealth(config.agentPort, 15000);
  let remoteNode = null;
  for (let i = 0; i < 10; i++) {
    remoteNode = await verifyRemoteRegistration(config.gatewayUrl, config.internalKey, config.nodeName, config.model).catch(() => null);
    if (remoteNode) break;
    await wait(500);
  }
  console.log(`✓ node-agent started in foreground mode (PID ${child.pid})`);
  console.log(`✓ Local health: ${health.status} registered=${health.registered}`);
  console.log(remoteNode ? '✓ Remote registration verified' : '• Remote registration not yet verified');
  console.log('Press Ctrl+C to stop.');

  const stopChild = () => {
    if (child.pid && isPidAlive(child.pid)) {
      child.kill('SIGTERM');
    }
  };
  process.on('SIGINT', stopChild);
  process.on('SIGTERM', stopChild);

  await new Promise<void>((resolve, reject) => {
    child.on('exit', async (code) => {
      log.end();
      await saveState({
        pid: child.pid,
        status: code === 0 ? 'stopped' : 'exited',
        agentPort: config.agentPort,
        binaryPath,
        logPath: paths.logFile,
        startedAt: new Date().toISOString(),
        stoppedAt: new Date().toISOString(),
        mode: 'foreground',
      });
      if (code === 0 || code === null) resolve();
      else reject(new Error(`node-agent exited with code ${code}`));
    });
    child.on('error', reject);
  });
}
