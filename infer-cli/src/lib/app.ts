// @ts-nocheck
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

import { ensureDir, fileExists, isExecutable, readJsonIfExists, writeJson } from './fs.js';
import { ollamaTags, getJson } from './http.js';
import { inferPaths } from './paths.js';
import { defaultNodeName, parseOllamaPort, platformTarget, validateLocalOllamaUrl } from './runtime.js';
import { isPidAlive, waitForHealth } from './process.js';

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../../..');

export async function loadConfig() {
  return await readJsonIfExists(inferPaths().config);
}

export async function loadState() {
  return await readJsonIfExists(inferPaths().state);
}

export async function saveConfig(config: any) {
  await writeJson(inferPaths().config, config);
}

export async function saveState(state: any) {
  await writeJson(inferPaths().state, state);
}

export function resolveRunConfig(model: string, flags: Record<string, any>, existing: any = {}) {
  const ollamaUrl = validateLocalOllamaUrl(String(flags['ollama-url'] || existing.ollamaUrl || 'http://127.0.0.1:11434'));
  const gatewayUrl = String(flags.gateway || existing.gatewayUrl || '').replace(/\/$/, '');
  const internalKey = String(flags.token || process.env.INFER_INTERNAL_KEY || existing.internalKey || '');
  const agentPort = Number(flags['agent-port'] || existing.agentPort || 8181);
  const nodeName = String(flags['node-name'] || existing.nodeName || defaultNodeName());
  const host = String(flags.host || existing.host || '127.0.0.1');
  const foreground = flags.foreground === true || flags.detach !== true;
  const detach = flags.detach === true;
  const ollamaPort = Number(flags['ollama-port'] || existing.ollamaPort || parseOllamaPort(ollamaUrl));
  if (!gatewayUrl) throw new Error('Missing --gateway for infer run');
  if (!internalKey) throw new Error('Missing --token for infer run (or INFER_INTERNAL_KEY env)');
  return {
    model,
    gatewayUrl,
    internalKey,
    ollamaUrl,
    ollamaPort,
    agentPort,
    nodeName,
    host,
    foreground,
    detach,
  };
}

export async function ensureInferDirs() {
  const paths = inferPaths();
  await ensureDir(paths.home);
  await ensureDir(paths.binDir);
  await ensureDir(paths.currentBinDir);
  await ensureDir(paths.logsDir);
  return paths;
}

export async function resolveNodeAgentBinary(flags: Record<string, any>) {
  const paths = await ensureInferDirs();
  if (flags['agent-bin']) {
    const supplied = path.resolve(String(flags['agent-bin']));
    if (!(await isExecutable(supplied))) throw new Error(`Provided --agent-bin is not executable: ${supplied}`);
    return supplied;
  }

  const currentBinary = path.join(paths.currentBinDir, process.platform === 'win32' ? 'node-agent.exe' : 'node-agent');
  if (await isExecutable(currentBinary)) {
    return currentBinary;
  }

  const target = platformTarget();
  const ext = process.platform === 'win32' ? '.exe' : '';
  const builtBinary = path.join(paths.binDir, 'local', target, `node-agent${ext}`);
  if (await isExecutable(builtBinary)) {
    await linkCurrentBinary(builtBinary);
    return builtBinary;
  }

  await ensureDir(path.dirname(builtBinary));
  await buildLocalNodeAgent(builtBinary);
  await linkCurrentBinary(builtBinary);
  return builtBinary;
}

async function linkCurrentBinary(binaryPath: string) {
  const paths = inferPaths();
  const linkPath = path.join(paths.currentBinDir, path.basename(binaryPath));
  await ensureDir(paths.currentBinDir);
  try {
    await fs.promises.rm(linkPath, { force: true });
  } catch {}
  await fs.promises.copyFile(binaryPath, linkPath);
  await fs.promises.chmod(linkPath, 0o755);
}

export async function buildLocalNodeAgent(outputPath: string) {
  const source = path.join(ROOT, 'cmd', 'node-agent');
  if (!(await fileExists(source))) {
    throw new Error(`Could not find local node-agent source at ${source}`);
  }
  await new Promise((resolve, reject) => {
    const child = spawn('go', ['build', '-o', outputPath, './cmd/node-agent'], {
      cwd: ROOT,
      stdio: 'inherit',
      env: process.env,
    });
    child.on('exit', (code) => {
      if (code === 0) resolve(undefined);
      else reject(new Error(`go build failed with exit code ${code}`));
    });
    child.on('error', reject);
  });
  await fs.promises.chmod(outputPath, 0o755);
}

export async function checkDoctor(config: any) {
  const paths = await ensureInferDirs();
  const checks: any[] = [];
  try {
    const temp = path.join(paths.home, '.write-check');
    await fs.promises.writeFile(temp, 'ok');
    await fs.promises.rm(temp, { force: true });
    checks.push({ ok: true, label: `Writable directory: ${paths.home}` });
  } catch (error: any) {
    checks.push({ ok: false, label: `Cannot write to ${paths.home}: ${error.message}` });
  }

  try {
    const ollama = validateLocalOllamaUrl(config.ollamaUrl || 'http://127.0.0.1:11434');
    const tags = await ollamaTags(ollama);
    if (!tags.ok) throw new Error(tags.text || `HTTP ${tags.status}`);
    checks.push({ ok: true, label: `Ollama reachable: ${ollama}` });
    if (config.model) {
      const models = Array.isArray(tags.body?.models) ? tags.body.models.map((m: any) => m.name) : [];
      const present = models.includes(config.model);
      checks.push({ ok: present, label: present ? `Model present in Ollama: ${config.model}` : `Model missing in Ollama: ${config.model}` });
    }
  } catch (error: any) {
    checks.push({ ok: false, label: `Ollama check failed: ${error.message}` });
  }

  if (config.gatewayUrl) {
    try {
      const gateway = await getJson(`${config.gatewayUrl}/health`);
      if (!gateway.ok) throw new Error(gateway.text || `HTTP ${gateway.status}`);
      checks.push({ ok: true, label: `Gateway reachable: ${config.gatewayUrl}` });
    } catch (error: any) {
      checks.push({ ok: false, label: `Gateway check failed: ${error.message}` });
    }
  }

  try {
    await resolveNodeAgentBinary({ 'agent-bin': config.agentBin });
    checks.push({ ok: true, label: 'node-agent binary available' });
  } catch (error: any) {
    checks.push({ ok: false, label: `node-agent binary unavailable: ${error.message}` });
  }

  return checks;
}

export async function verifyRemoteRegistration(gatewayUrl: string, internalKey: string, nodeName: string, model: string) {
  const response = await getJson(`${gatewayUrl}/v1/internal/nodes`, {
    headers: { Authorization: `Bearer ${internalKey}` },
  });
  if (!response.ok) {
    throw new Error(response.text || `HTTP ${response.status}`);
  }
  const nodes = Array.isArray(response.body?.data) ? response.body.data : [];
  return nodes.find((node: any) => node.name === nodeName && node.model === model) || null;
}

export async function getRuntimeStatus() {
  const config = await loadConfig();
  const state = await loadState();
  const running = isPidAlive(state?.pid);
  let localHealth = null;
  if (running && state?.agentPort) {
    try {
      localHealth = await waitForHealth(state.agentPort, 1500);
    } catch {}
  }
  let remoteNode = null;
  if (config?.gatewayUrl && config?.internalKey && config?.nodeName && config?.model) {
    try {
      remoteNode = await verifyRemoteRegistration(config.gatewayUrl, config.internalKey, config.nodeName, config.model);
    } catch {}
  }
  return { config, state, running, localHealth, remoteNode };
}

export function printChecks(checks: any[]) {
  for (const check of checks) {
    console.log(`${check.ok ? '✓' : '✗'} ${check.label}`);
  }
}

export function projectRoot() {
  return ROOT;
}
