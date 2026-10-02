#!/usr/bin/env node

const { readFile, writeFile } = require("node:fs/promises");
const path = require("node:path");
const toml = require("@iarna/toml");

function sanitizeText(value) {
  if (typeof value !== "string") {
    return value;
  }

  return value
    .replace(/\r\n?/g, "\n")
    .replace(/[ \t]+$/gm, "")
    .replace(/[ \t]+(?=\n)/g, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function normalizeErrorCode(value) {
  if (typeof value === "number") {
    return Number.isSafeInteger(value) ? value : String(value);
  }

  if (typeof value === "string") {
    const trimmed = value.trim();
    if (/^-?\d+$/.test(trimmed)) {
      const parsed = BigInt(trimmed);
      return parsed > BigInt(Number.MAX_SAFE_INTEGER) ||
        parsed < BigInt(-Number.MAX_SAFE_INTEGER)
        ? trimmed
        : Number(parsed);
    }

    return trimmed;
  }

  if (typeof value === "bigint") {
    return value.toString();
  }

  return value;
}

function sanitizeAndNormalize(value, key = "") {
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeAndNormalize(item, key));
  }

  if (value && typeof value === "object") {
    return Object.entries(value).reduce((acc, [entryKey, entryValue]) => {
      const nextKey = key ? `${key}.${entryKey}` : entryKey;
      let normalizedValue = entryValue;

      if (entryKey === "code" || entryKey === "error_code") {
        normalizedValue = normalizeErrorCode(entryValue);
      } else if (
        typeof entryValue === "string" &&
        /description|summary|explanation|name|severity|category|difficulty|likelihood|source|id/i.test(
          entryKey,
        )
      ) {
        normalizedValue = sanitizeText(entryValue);
      } else if (entryValue && typeof entryValue === "object") {
        normalizedValue = sanitizeAndNormalize(entryValue, nextKey);
      }

      acc[entryKey] = normalizedValue;
      return acc;
    }, {});
  }

  if (
    typeof value === "string" &&
    /description|summary|explanation|name|severity|category|difficulty|likelihood|source|id/i.test(
      key,
    )
  ) {
    return sanitizeText(value);
  }

  return value;
}

const NUMERIC_KEY_PATTERN = /^-?\d+$/;

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function resolveErrorCode(node, key) {
  const candidates = [node.code, node.error_code, key];

  for (const candidate of candidates) {
    if (candidate === undefined || candidate === null || candidate === "") {
      continue;
    }

    const normalized = normalizeErrorCode(candidate);
    if (typeof normalized === "number") {
      return normalized;
    }

    if (
      typeof normalized === "string" &&
      NUMERIC_KEY_PATTERN.test(normalized)
    ) {
      return normalized;
    }
  }

  return null;
}

function isErrorDefinition(node, key) {
  return (
    isPlainObject(node) &&
    typeof node.name === "string" &&
    resolveErrorCode(node, key) !== null
  );
}

function flattenTaxonomy(root, options = {}) {
  const dictionary = {};
  const warnings = [];

  const warn = (path, message) => {
    warnings.push({ path, message });
    if (typeof options.onWarning === "function") {
      options.onWarning(`[flatten-taxonomy] ${path}: ${message}`);
    }
  };

  const addDefinition = (node, key, path) => {
    const code = resolveErrorCode(node, key);
    const dictionaryKey = String(code);

    if (Object.prototype.hasOwnProperty.call(dictionary, dictionaryKey)) {
      warn(
        path,
        `duplicate error code ${dictionaryKey}; keeping the first definition encountered`,
      );
      return;
    }

    dictionary[dictionaryKey] = { ...node, code };
  };

  const visit = (node, key, path) => {
    if (Array.isArray(node)) {
      node.forEach((item, index) => {
        const itemPath = `${path}[${index}]`;

        if (isPlainObject(item) || Array.isArray(item)) {
          visit(item, key, itemPath);
        }
        // Primitive array entries (e.g. related_errors string lists) are plain
        // metadata attached to a definition — nothing to flatten, skip quietly.
      });
      return;
    }

    if (!isPlainObject(node)) {
      return;
    }

    if (isErrorDefinition(node, key)) {
      addDefinition(node, key, path);
      return;
    }

    for (const [childKey, childValue] of Object.entries(node)) {
      const childPath = path ? `${path}.${childKey}` : childKey;

      if (NUMERIC_KEY_PATTERN.test(childKey) && !isPlainObject(childValue)) {
        // A numeric key promises an error definition table. Anything else
        // (an array, a bare string, …) is a schema deviation from upstream:
        // skip it instead of letting a property access blow up downstream.
        warn(
          childPath,
          `expected an error definition table but found ${describeShape(childValue)}; node skipped`,
        );
        continue;
      }

      if (isPlainObject(childValue) || Array.isArray(childValue)) {
        visit(childValue, childKey, childPath);
      }
    }
  };

  visit(root, "", "");

  return { dictionary, warnings };
}

function describeShape(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return "an array";
  return `a ${typeof value}`;
}

async function buildTaxonomyJson({ inputPath, outputPath, onWarning }) {
  let input;
  try {
    input = await readFile(inputPath, "utf8");
  } catch (error) {
    throw new Error(formatReadError(error, inputPath));
  }

  let parsed;
  try {
    parsed = toml.parse(input);
  } catch (error) {
    throw new Error(
      `Parse Error: The TOML file exists but contains invalid syntax:\n  ${error.message}`,
    );
  }

  const sanitized = sanitizeAndNormalize(parsed);
  const { dictionary, warnings } = flattenTaxonomy(sanitized, { onWarning });
  const serialized = JSON.stringify(dictionary, null, 2) + "\n";

  try {
    await writeFile(outputPath, serialized, "utf8");
  } catch (error) {
    throw new Error(
      `Unexpected file system error while writing the taxonomy JSON file:\n` +
        `  Code: ${error.code || "unknown"}\n` +
        `  Message: ${error.message}`,
    );
  }

  return { serialized, dictionary, warnings };
}

function formatReadError(error, inputPath) {
  if (error.code === "ENOENT") {
    return (
      `Critical Error: The core taxonomy TOML file could not be located at the expected path:\n` +
      `  ${inputPath}\n\n` +
      `Please ensure the crates/core submodule is initialized:\n` +
      `  git submodule update --init --recursive`
    );
  }

  if (error.code === "EACCES") {
    return (
      `Permission Error: Cannot read the taxonomy TOML file due to a permission restriction:\n` +
      `  ${inputPath}\n\n` +
      `Verify file permissions or check if another process has locked the file.`
    );
  }

  return (
    `Unexpected file system error while reading the taxonomy TOML file:\n` +
    `  Code: ${error.code || "unknown"}\n` +
    `  Message: ${error.message}`
  );
}

async function main() {
  const workspaceRoot = path.resolve(__dirname, "..", "..", "..");
  const inputPath = path.join(
    workspaceRoot,
    "crates",
    "core",
    "src",
    "taxonomy",
    "data",
    "contract.toml",
  );
  const outputPath = path.join(__dirname, "taxonomy.json");

  const { serialized, dictionary } = await buildTaxonomyJson({
    inputPath,
    outputPath,
    onWarning: (message) => console.warn(message),
  });
  const entryCount = Object.keys(dictionary).length;
  console.log(
    `Wrote flattened taxonomy JSON (${entryCount} error codes) to ${path.relative(workspaceRoot, outputPath)}`,
  );
  return serialized;
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}

module.exports = {
  sanitizeText,
  normalizeErrorCode,
  sanitizeAndNormalize,
  isPlainObject,
  isErrorDefinition,
  resolveErrorCode,
  flattenTaxonomy,
  buildTaxonomyJson,
  formatReadError,
};
