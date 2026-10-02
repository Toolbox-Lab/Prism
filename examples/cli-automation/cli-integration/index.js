const { execFile } = require("child_process");
const path = require("path");

// Base64 validation regex (strict, no whitespace allowed, common safe charset)
const BASE64_REGEX = /^[A-Za-z0-9+/]+={0,2}$/;

function isValidBase64(str) {
  if (typeof str !== "string" || str.length === 0) {
    return false;
  }
  // Must be multiple of 4 in length for valid padding
  if (str.length % 4 !== 0) {
    return false;
  }
  return BASE64_REGEX.test(str);
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 0) {
    console.error("Usage: node index.js <base64_xdr_string>");
    process.exit(1);
  }

  const xdrString = args[0];

  if (!isValidBase64(xdrString)) {
    console.error("Error: Invalid base64 XDR string provided.");
    process.exit(1);
  }

  // Path to the Rust binary (relative to example, assuming debug build)
  const gratBinary = path.resolve(__dirname, "../../../target/debug/grat");

  // Use execFile with immutable args array - NO SHELL
  const childArgs = ["decode", xdrString, "format", "json"];

  const child = execFile(gratBinary, childArgs, (error, stdout, stderr) => {
    if (error) {
      console.error("Execution error:", error.message);
      process.exit(1);
    }
    if (stderr) {
      console.error(stderr.trim());
    }
    console.log(stdout.trim());
  });
}

main();
