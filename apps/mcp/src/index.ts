#!/usr/bin/env node
import { createServer as createHttpServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { randomUUID } from 'node:crypto';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import { isInitializeRequest } from '@modelcontextprotocol/sdk/types.js';
import { createServer } from './server.js';

// All logging goes to stderr — stdout is reserved for the stdio JSON-RPC stream.
const BASE_URL = process.env.BASE_URL || 'http://localhost:4320';
const ENV_API_KEY = process.env.API_KEY;
const MODE = (process.env.MCP_TRANSPORT || (process.argv.includes('--http') ? 'http' : 'stdio')).toLowerCase();

function bearer(req: IncomingMessage): string | undefined {
  const auth = req.headers['authorization'];
  const m = typeof auth === 'string' ? auth.match(/^Bearer\s+(.+)$/i) : null;
  return m?.[1]?.trim();
}

function readBody(req: IncomingMessage): Promise<unknown> {
  return new Promise((resolve) => {
    let data = '';
    req.on('data', (c) => (data += c));
    req.on('end', () => {
      if (!data) return resolve(undefined);
      try {
        resolve(JSON.parse(data));
      } catch {
        resolve(undefined);
      }
    });
    req.on('error', () => resolve(undefined));
  });
}

function rpcError(res: ServerResponse, status: number, message: string) {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ jsonrpc: '2.0', error: { code: -32000, message }, id: null }));
}

async function runStdio() {
  if (!ENV_API_KEY) {
    console.error('[task-timer-mcp] Missing API_KEY env var (mint one at /dashboard/keys).');
    process.exit(1);
  }
  const server = createServer(BASE_URL, ENV_API_KEY);
  await server.connect(new StdioServerTransport());
  console.error(`[task-timer-mcp] stdio connected (BASE_URL=${BASE_URL})`);
}

function runHttp() {
  const port = Number(process.env.PORT || 3010);
  const host = process.env.HOST || '127.0.0.1';
  const path = process.env.MCP_PATH || '/mcp';

  // One transport per MCP session id.
  const transports = new Map<string, StreamableHTTPServerTransport>();

  const http = createHttpServer(async (req, res) => {
    const url = new URL(req.url ?? '/', `http://${req.headers.host ?? host}`);
    if (url.pathname !== path) {
      rpcError(res, 404, 'Not found');
      return;
    }

    // Each client authenticates with its own Task Timer API key (multi-tenant);
    // falls back to the server's API_KEY env for single-user hosting.
    const apiKey = bearer(req) ?? ENV_API_KEY;
    if (!apiKey) {
      rpcError(res, 401, 'Missing API key (send Authorization: Bearer <task-timer-key>)');
      return;
    }

    const sessionId = req.headers['mcp-session-id'];
    const sid = Array.isArray(sessionId) ? sessionId[0] : sessionId;
    let transport = sid ? transports.get(sid) : undefined;

    const body = req.method === 'POST' ? await readBody(req) : undefined;

    if (!transport) {
      if (req.method === 'POST' && isInitializeRequest(body)) {
        transport = new StreamableHTTPServerTransport({
          sessionIdGenerator: () => randomUUID(),
          onsessioninitialized: (newId) => {
            transports.set(newId, transport!);
          }
        });
        transport.onclose = () => {
          if (transport!.sessionId) transports.delete(transport!.sessionId);
        };
        // Bind this session's server to the key presented on initialize.
        await createServer(BASE_URL, apiKey).connect(transport);
      } else {
        rpcError(res, 400, 'No valid session — send an initialize request first.');
        return;
      }
    }

    await transport.handleRequest(req, res, body);
  });

  http.listen(port, host, () => {
    console.error(`[task-timer-mcp] http listening on http://${host}:${port}${path} (BASE_URL=${BASE_URL})`);
  });
}

if (MODE === 'http') {
  runHttp();
} else {
  await runStdio();
}
