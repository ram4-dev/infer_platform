// @ts-nocheck
import fs from 'node:fs';
import path from 'node:path';

export async function ensureDir(dir: string) {
  await fs.promises.mkdir(dir, { recursive: true });
}

export async function writeJson(file: string, value: any) {
  await ensureDir(path.dirname(file));
  await fs.promises.writeFile(file, JSON.stringify(value, null, 2) + '\n', 'utf8');
}

export async function readJson(file: string) {
  const raw = await fs.promises.readFile(file, 'utf8');
  return JSON.parse(raw);
}

export async function readJsonIfExists(file: string) {
  try {
    return await readJson(file);
  } catch (error: any) {
    if (error?.code === 'ENOENT') return null;
    throw error;
  }
}

export async function fileExists(file: string) {
  try {
    await fs.promises.access(file, fs.constants.F_OK);
    return true;
  } catch {
    return false;
  }
}

export async function isExecutable(file: string) {
  try {
    await fs.promises.access(file, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}
