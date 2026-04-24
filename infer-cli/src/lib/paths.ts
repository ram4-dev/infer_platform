// @ts-nocheck
import os from 'node:os';
import path from 'node:path';

export function inferPathsForHome(homeDir: string) {
  const home = path.join(homeDir, '.infer');
  return {
    home,
    binDir: path.join(home, 'bin'),
    currentBinDir: path.join(home, 'bin', 'current'),
    logsDir: path.join(home, 'logs'),
    config: path.join(home, 'config.json'),
    state: path.join(home, 'state.json'),
    logFile: path.join(home, 'logs', 'node-agent.log'),
  };
}

export function inferPaths() {
  return inferPathsForHome(os.homedir());
}
