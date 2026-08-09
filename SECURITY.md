# Security Policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes |
| Older versions | No |

## Report a vulnerability

Use [GitHub's private security advisory
form](https://github.com/Tinkora/image_to_icns/security/advisories/new). Do not
open a public issue or Discussion for a vulnerability.

Include affected versions, reproduction steps, impact, and any suggested
mitigation. Maintainers will acknowledge the report through the private
advisory and coordinate remediation and disclosure there. No paid bug bounty is
currently offered.

## Security model

The static editor processes source images and generated ICNS bytes in browser
memory. It does not contain upload code and does not require a backend.

The optional Session Worker stores metadata only:

- a 64-character random hexadecimal Session ID;
- a SHA-256 hash of a 128-character random hexadecimal secret;
- timestamps, source-format hint, state, output byte length, and representation
  count, plus a non-sensitive error code for failed conversions.

The plaintext secret is carried in the editor URL fragment. URL fragments are
not sent to the web host, Worker, or referrer targets. State-changing Worker
requests send the secret in a JSON body over HTTPS. Session records expire
after 30 minutes, and terminal records are deleted after 24 hours.

## In scope

- Source-image or generated-file disclosure
- Image parser memory-safety or denial-of-service issues
- ICNS output validation bypasses
- Session secret disclosure, prediction, or authorization bypass
- Worker input validation, D1 injection, or rate-limit bypass
- MCP JSON-RPC boundary or Worker URL handling vulnerabilities
- Dependency and GitHub Actions supply-chain vulnerabilities

Reports about an independently operated Worker should identify whether the
issue is in this repository or in the operator's deployment configuration.
