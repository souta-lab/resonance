# Security policy

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x | ✅ |

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Use GitHub's
private vulnerability reporting:

<https://github.com/souta-lab/resonance/security/advisories/new>

Include a description, reproduction steps, and the affected version. You can
expect an acknowledgement within a few days.

## Scope

Resonance runs as your user and binds to `127.0.0.1` by default. The relevant
attack surface is:

- The HTTP/WebSocket control API (session token, `Origin` checks, revision
  checks).
- Preset and parameter parsing (JSON size, types, ranges).
- Asset resolution for impulse responses and models (path traversal).
- The `pw-cat` / `pw-cli` subprocess bridge.

## What we do

- Mutations require `X-Resonance-Token`, issued only to same-origin requests.
- WebSocket connections require the token in the query string.
- `Origin` is validated on mutations and on the session endpoint.
- Asset names containing `/`, `\` or `..` are rejected; resolution is limited
  to the configured directories.
- Preset input is type-checked and range-checked before it reaches the DSP.
- The daemon never logs tokens or passwords.

## What we do not do (yet)

- TLS termination. If you expose the daemon beyond localhost, put it behind a
  reverse proxy with authentication and TLS.
- Sandboxing of the `pw-cat` subprocesses.
