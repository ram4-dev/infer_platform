// @ts-nocheck
import fs from 'node:fs';
import { spawn } from 'node:child_process';

export function isPidAlive(pid?: number) {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export async function wait(ms: number) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitForHealth(agentPort: number, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  let lastError: any = null;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${agentPort}/health`);
      if (response.ok) {
        return await response.json();
      }
    } catch (error) {
      lastError = error;
    }
    await wait(500);
  }
  throw new Error(`Timed out waiting for node-agent health on port ${agentPort}${lastError ? `: ${lastError}` : ''}`);
}

export function spawnDetached(command: string, args: string[], env: Record<string, string>, logFile: string) {
  const fd = fs.openSync(logFile, 'a');
  const child = spawn(command, args, {
    env: { ...process.env, ...env },
    detached: true,
    stdio: ['ignore', fd, fd],
  });
  child.unref();
  fs.closeSync(fd);
  return child;
}

export function spawnForeground(command: string, args: string[], env: Record<string, string>, logFile: string) {
  const log = fs.createWriteStream(logFile, { flags: 'a' });
  const child = spawn(command, args, {
    env: { ...process.env, ...env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  child.stdout?.on('data', (chunk) => {
    process.stdout.write(chunk);
    log.write(chunk);
  });
  child.stderr?.on('data', (chunk) => {
    process.stderr.write(chunk);
    log.write(chunk);
  });

  return { child, log };
}
