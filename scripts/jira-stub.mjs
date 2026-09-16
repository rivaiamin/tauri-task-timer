// Stub JIRA REST API for scripts/verify-jira-stop.mjs.
//
// Runs as its own process on purpose: the verifier drives the TUI with
// spawnSync, which blocks the parent's event loop, so an in-process stub would
// be unable to accept the connections it is supposed to record.
//
//   node scripts/jira-stub.mjs
//
// Prints `PORT <n>` on stdout once listening, then answers with the recorded
// request log as `REQS <json>` when it receives SIGUSR2, or on a port-file
// append when started with `--capture <path>`.

import { createServer } from 'node:http';
import { appendFileSync } from 'node:fs';

// A workflow whose statuses are all reachable from each other, mirroring the
// production AIMSIS board's shape.
const TRANSITIONS = { '11': 'To Do', '21': 'In Progress', '51': 'Cek di Local' };

const captureIndex = process.argv.indexOf('--capture');
const capturePath = captureIndex === -1 ? null : process.argv[captureIndex + 1];

const requests = [];

const server = createServer((req, res) => {
  let body = '';
  req.on('data', (chunk) => {
    body += chunk;
  });
  req.on('end', () => {
    const path = req.url.split('?')[0];
    let parsed = null;
    try {
      parsed = body ? JSON.parse(body) : null;
    } catch {
      parsed = { unparseable: body };
    }
    const entry = { method: req.method, path, body: parsed };
    requests.push(entry);
    if (capturePath) appendFileSync(capturePath, `${JSON.stringify(entry)}\n`);

    if (path.endsWith('/transitions') && req.method === 'GET') {
      const payload = JSON.stringify({
        transitions: Object.entries(TRANSITIONS).map(([id, name]) => ({ id, name, to: { id, name } })),
      });
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(payload);
      return;
    }
    res.writeHead(204); // POST worklog / POST transitions
    res.end();
  });
});

server.listen(0, '127.0.0.1', () => {
  console.log(`PORT ${server.address().port}`);
});

process.on('SIGUSR2', () => {
  console.log(`REQS ${JSON.stringify(requests)}`);
});

// Bounded lifetime so a crashed verifier cannot leave this process behind.
setTimeout(() => process.exit(0), 120_000);
