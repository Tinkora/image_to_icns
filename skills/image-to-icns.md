# image_to_icns MCP integration

Use the optional MCP server to create and observe a browser-based macOS ICNS
conversion Session. The MCP server requires a self-hosted Session Worker.

## Workflow

1. Call `create_icns_session`, optionally with `source_format` set to `png`,
   `jpeg`, or `svg`.
2. Give the returned `editor_url` to the user.
3. The user opens the editor, imports an image, adjusts the crop, generates the
   ICNS file, and downloads it locally.
4. Call `query_icns_session` after the user returns. A completed response
   includes `output_byte_len` and `representation_count`.
5. Call `cancel_icns_session` only when the user abandons an active Session.

The generated file remains on the user's device. The MCP server cannot retrieve
or attach it to the conversation.

## Privacy and security

- Source images and generated files never enter the Worker.
- Sessions expire after 30 minutes.
- Session IDs contain 32 random bytes encoded as 64 hexadecimal characters.
- Secrets contain 64 random bytes encoded as 128 hexadecimal characters.
- Only a SHA-256 secret hash is stored in D1.
- The plaintext secret is carried in the editor URL fragment, not its query.
- State-changing requests require the secret.

See [the self-hosting guide](../docs/SELF_HOSTING.md) for deployment and
retention details.
