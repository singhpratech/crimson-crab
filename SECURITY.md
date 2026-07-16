# Security Policy

## Supported versions
The latest published release on crates.io.

## Reporting a vulnerability
Please do **not** open a public issue for security problems. Use GitHub's private vulnerability reporting ("Report a vulnerability" under the Security tab) on this repository. You'll get a response within 72 hours.

Notes for researchers: this crate never logs API keys, sends requests only to the configured `base_url`, and uses rustls by default. Anything violating that is a bug we want to hear about.
