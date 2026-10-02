# CLI Integration

This example demonstrates how an external Node.js process (like an IDE Extension, Web App backend, or CI Pipeline) can programmatically execute the `grat` Rust binary via `child_process.execFile` and parse the raw JSON outputs returned by standard I/O.

## Installation

No external dependencies required.

## Running the Example

Make sure the Rust core has been built locally first (`cargo build` in the monorepo root) so that `grat.exe` (or `grat` on unix) exists in `../../target/debug/grat`.

Run the script by providing a base64 XDR string:

```bash
npm start <xdr_string>
```
