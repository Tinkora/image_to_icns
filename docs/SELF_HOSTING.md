# Self-hosting the Session Worker

The browser editor does not require a backend. Deploy the Session Worker only
when an MCP client needs to create short-lived editor links and query their
status.

The Worker stores metadata in Cloudflare D1. It does not accept image uploads,
store generated `.icns` files, or require R2.

## Requirements

- A Cloudflare account with Workers and D1 enabled
- Node.js 20 or newer
- Rust 1.95 with the `wasm32-unknown-unknown` target
- `worker-build`

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build --locked
cd crates/image_to_icns_worker
cp wrangler.example.toml wrangler.toml
```

`wrangler.toml` is intentionally ignored by Git. Keep deployment-specific IDs
and URLs in that local file.

## Create the database

Authenticate Wrangler and create the D1 database:

```bash
npx wrangler login
npx wrangler d1 create image-to-icns-sessions
```

Put the returned database ID in the `database_id` field of `wrangler.toml`.

Apply the schema locally:

```bash
npx wrangler d1 migrations apply image-to-icns-sessions --local
```

## Run locally

Build and serve the editor from the repository root:

```bash
./scripts/build_web.sh
python3 -m http.server 4173 --directory dist
```

The default `dist/config.js` disables Session callbacks. For local development,
set its trusted deployment value before opening a Session link:

```javascript
globalThis.__ICNS_WORKER_URL__ = "http://127.0.0.1:8787";
```

In another terminal, from `crates/image_to_icns_worker`:

```bash
npx wrangler dev
```

Create a Session:

```bash
curl --request POST http://127.0.0.1:8787/sessions \
  --header 'Content-Type: application/json' \
  --data '{"source_format":"png"}'
```

The returned `editor_url` contains the Session ID, secret, and Worker origin in
the URL fragment. Open it in a browser. The editor accepts the link only when
its Worker origin matches the trusted `config.js` value, then reports `editing`
when it loads and `completed` after the generated file is downloaded.

## Deploy

Set `EDITOR_BASE_URL` in `wrangler.toml` to the HTTPS origin and path of the
deployed editor. Set the deployed editor's `config.js` to the exact public
Worker origin using HTTPS:

```javascript
globalThis.__ICNS_WORKER_URL__ = "https://your-worker.example.com";
```

The repository default is `null` and is what Tinkora Pages publishes. Treat
`config.js` as trusted deployment code: do not derive it from URL parameters or
other request data. Then apply the migration and deploy:

```bash
npx wrangler d1 migrations apply image-to-icns-sessions --remote
npx wrangler deploy
```

Use the deployed Worker URL when starting the MCP server:

```bash
image_to_icns_mcp --worker-url https://your-worker.example.com
```

The Worker derives its public origin from each create request and places that
origin in the editor URL. The value is an equality check for the editor's
trusted deployment configuration, not a way for a link to choose a request
target.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/sessions` | Create a 30-minute Session |
| `GET` | `/sessions/:id` | Read Session state and completion metadata |
| `PATCH` | `/sessions/:id` | Update state using the Session secret |
| `DELETE` | `/sessions/:id` | Mark an active Session as cancelled |

Mutating requests require the 128-character hexadecimal secret. Session IDs
are 64-character hexadecimal values. The Worker accepts cross-origin browser
requests because the editor and Worker are normally deployed on different
origins; possession of the high-entropy secret authorizes state changes.

## Retention

- Active Sessions expire after 30 minutes.
- A scheduled task runs every five minutes.
- Terminal records are retained for no more than 24 hours before deletion.
- Rate-limit counters use 60-second windows.

The default limit is 30 requests per IP address per minute. A D1 failure causes
the Worker to fail closed with HTTP 503 rather than bypass the limit.
