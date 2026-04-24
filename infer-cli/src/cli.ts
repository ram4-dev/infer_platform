// @ts-nocheck
import { parseCli } from './lib/args.js';
import { doctorCommand } from './commands/doctor.js';
import { logsCommand } from './commands/logs.js';
import { runCommand } from './commands/run.js';
import { statusCommand } from './commands/status.js';
import { stopCommand } from './commands/stop.js';

function printHelp() {
  console.log(`infer CLI (internal MVP)\n\nUsage:\n  infer run <model> --gateway <url> --token <internal-key> [--detach|--foreground]\n  infer status\n  infer stop\n  infer doctor [model] [--gateway <url>] [--ollama-url <url>]\n  infer logs [--follow]\n\nNotes:\n  - This MVP is for internal/trusted alpha testers.\n  - Ollama must be local (localhost/127.0.0.1).\n  - If no node-agent binary is available, the CLI builds it locally from ./cmd/node-agent.\n  - Local state lives in ~/.infer\n`);
}

async function main() {
  const parsed = parseCli(process.argv.slice(2));
  switch (parsed.command) {
    case 'run':
      await runCommand(parsed.flags, parsed.positionals);
      break;
    case 'status':
      await statusCommand();
      break;
    case 'stop':
      await stopCommand();
      break;
    case 'doctor':
      await doctorCommand(parsed.flags, parsed.positionals);
      break;
    case 'logs':
      await logsCommand(parsed.flags);
      break;
    case 'help':
    case '--help':
    case '-h':
    default:
      printHelp();
      break;
  }
}

main().catch((error) => {
  console.error(`infer error: ${error.message}`);
  process.exit(1);
});
