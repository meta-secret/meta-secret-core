import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const COUNTER_FILE = path.join(__dirname, '..', '.run-counter');

export function getNextRunNumber(): number {
  let current = 0;

  if (fs.existsSync(COUNTER_FILE)) {
    const content = fs.readFileSync(COUNTER_FILE, 'utf-8').trim();
    const parsed = parseInt(content, 10);
    if (!isNaN(parsed)) {
      current = parsed;
    }
  }

  const next = current + 1;
  fs.writeFileSync(COUNTER_FILE, next.toString(), 'utf-8');
  return next;
}
