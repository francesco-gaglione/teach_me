# Security policy

teach_me stores user passwords (hashed) and users' LLM API keys (encrypted) on a self-hosted server, so security reports are taken seriously.

## Reporting a vulnerability

**Please do not open a public issue.** Use GitHub's private reporting instead: the **Security** tab of this repository, then **Report a vulnerability**.

Include what you found, how to reproduce it and the version (commit) affected. You will get an answer as soon as possible; this is a personal project, so there is no formal response time.

## Supported versions

Only the latest version on `main` is supported.

## Deployment notes

- The server is designed to be reached through Tailscale (encrypted tunnel, no TLS of its own). Publishing it on a LAN is plain HTTP: see [`backend/README.md`](backend/README.md).
- Keep `TEACH_ME_SECRET_KEY` out of backups and never commit `.env`.
