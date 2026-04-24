// @ts-nocheck
import fs from 'node:fs';

import { inferPaths } from '../lib/paths.js';
import { wait } from '../lib/process.js';

export async function logsCommand(flags: Record<string, any>) {
  const file = inferPaths().logFile;
  if (!fs.existsSync(file)) {
    console.log(`No logs found at ${file}`);
    return;
  }

  const follow = flags.follow === true;
  const content = await fs.promises.readFile(file, 'utf8');
  process.stdout.write(content);
  if (!follow) return;

  let offset = Buffer.byteLength(content);
  console.log('\n-- following logs --');
  while (true) {
    const stat = await fs.promises.stat(file);
    if (stat.size > offset) {
      const stream = fs.createReadStream(file, { start: offset, end: stat.size });
      for await (const chunk of stream) {
        process.stdout.write(chunk);
      }
      offset = stat.size;
    }
    await wait(500);
  }
}
