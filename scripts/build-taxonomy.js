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
        warn(
          childPath,
          `expected an error definition table but found ${typeof childValue}; node skipped`,
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

async function buildTaxonomyJson({ inputPath, outputPath, onWarning }) {
  let input;
  try {
    input = await readFile(inputPath, "utf8");
  } catch (error) {
    throw new Error(`Could not read taxonomy TOML file: ${inputPath}`);
  }

  let parsed;
  try {
    parsed = toml.parse(input);
  } catch (error) {
    throw new Error(`Could not parse TOML file: ${error.message}`);
  }

  const sanitized = sanitizeAndNormalize(parsed);
  const { dictionary, warnings } = flattenTaxonomy(sanitized, { onWarning });
  const serialized = JSON.stringify(dictionary, null, 2) + "\n";

  try {
    await writeFile(outputPath, serialized, "utf8");
  } catch (error) {
    throw new Error(`Could not write taxonomy JSON file: ${outputPath}`);
  }

  return { serialized, dictionary, warnings };
}

async function main() {
  const workspaceRoot = path.resolve(__dirname, "..");
  const inputPath = path.join(
    workspaceRoot,
    "crates",
    "core",
    "src",
    "taxonomy",
    "data",
    "contract.toml",
  );
  const outputPath = path.join(
    workspaceRoot,
    "apps",
    "web",
    "src",
    "lib",
    "contract-taxonomy.json",
  );

  const { dictionary } = await buildTaxonomyJson({
    inputPath,
    outputPath,
    onWarning: (message) => console.warn(message),
  });
  const entryCount = Object.keys(dictionary).length;
  console.log(
    `Generated contract taxonomy JSON (${entryCount} errors) at ${path.relative(workspaceRoot, outputPath)}`,
  );
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
};
