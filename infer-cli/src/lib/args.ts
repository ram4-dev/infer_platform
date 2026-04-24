// @ts-nocheck
export function parseCli(argv: string[]) {
  const [command = 'help', ...rest] = argv;
  const positionals: string[] = [];
  const flags: Record<string, any> = {};

  for (let i = 0; i < rest.length; i++) {
    const token = rest[i];
    if (!token.startsWith('--')) {
      positionals.push(token);
      continue;
    }
    const key = token.slice(2);
    const next = rest[i + 1];
    if (!next || next.startsWith('--')) {
      flags[key] = true;
      continue;
    }
    flags[key] = next;
    i++;
  }

  return { command, positionals, flags };
}

export function flagString(flags: Record<string, any>, key: string, fallback?: string) {
  const value = flags[key];
  if (typeof value === 'string' && value.length > 0) return value;
  return fallback;
}

export function flagNumber(flags: Record<string, any>, key: string, fallback?: number) {
  const value = flags[key];
  if (typeof value === 'string' && value.length > 0) {
    const parsed = Number(value);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return fallback;
}

export function flagBool(flags: Record<string, any>, key: string) {
  return flags[key] === true;
}
