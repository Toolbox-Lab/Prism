# Security Policy

## Reporting a Vulnerability

We prioritize the security of our users. If you find a security vulnerability in Grat, please report it privately.

### How to Report

Please do NOT open public issues for security vulnerabilities. Send an email to the project maintainers.

### Security Best Practices

- **Read-Only Operations**: Grat is a read-only diagnostic tool and never handles private keys or signs transactions.
- **Isolated Sandbox**: Transaction replay occurs in an isolated local sandbox with no network access.
- **Validation**: All external inputs (XDR, WASM metadata) are validated before processing.
