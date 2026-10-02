const test = require("node:test");
const assert = require("node:assert/strict");
const os = require("node:os");
const path = require("node:path");
const {
  sanitizeText,
  normalizeErrorCode,
  flattenTaxonomy,
  buildTaxonomyJson,
} = require("./index");

test("sanitizeText trims and normalizes pesky whitespace", () => {
  const value = "  Contract error\r\nwith trailing spaces   \r\n";
  assert.equal(sanitizeText(value), "Contract error\nwith trailing spaces");
});

test("normalizeErrorCode preserves unsafe integers without losing precision", () => {
  assert.equal(normalizeErrorCode(42), 42);
  assert.equal(normalizeErrorCode("9007199254740993"), "9007199254740993");
  assert.equal(
    normalizeErrorCode(BigInt("9007199254740993")),
    "9007199254740993",
  );
});

test("flattenTaxonomy flattens array-of-tables definitions keyed by their code field", () => {
  const ast = {
    category: {
      name: "Contract",
      description: "Errors raised by contract code.",
    },
    errors: [
      { code: 0, name: "ContractError", summary: "General contract error." },
      {
        code: 6,
        name: "AccountMissingError",
        summary: "Account does not exist.",
      },
    ],
  };

  const { dictionary, warnings } = flattenTaxonomy(ast);

  assert.deepEqual(Object.keys(dictionary).sort(), ["0", "6"]);
  assert.equal(dictionary["0"].name, "ContractError");
  assert.equal(dictionary["6"].summary, "Account does not exist.");
  assert.equal(warnings.length, 0);
});

test("flattenTaxonomy resolves definitions nested arbitrarily deep in namespace sections", () => {
  const ast = {
    errors: {
      contract: {
        authentication: {
          1042: {
            name: "InvalidSignature",
            description: "The signature check failed.",
          },
        },
        storage: {
          ledger: {
            2077: {
              name: "EntryExpired",
              description: "The ledger entry has expired.",
            },
          },
        },
      },
    },
  };

  const { dictionary, warnings } = flattenTaxonomy(ast);

  assert.equal(dictionary["1042"].name, "InvalidSignature");
  assert.equal(dictionary["1042"].code, 1042);
  assert.equal(dictionary["2077"].description, "The ledger entry has expired.");
  assert.equal(warnings.length, 0);
});

test("flattenTaxonomy skips anomalous shapes instead of crashing", () => {
  const ast = {
    errors: {
      contract: {
        // Upstream regression: a generic array where a definition table belongs.
        1042: ["not", "a", "table"],
        // A bare primitive under a numeric key.
        1043: "just a string",
        // Structural junk that must not blow up the walker.
        weird: null,
        deeper: {
          1044: {
            name: "StillWorks",
            description: "Valid sibling of anomalous nodes.",
          },
        },
      },
      mixed_array: [
        null,
        "metadata",
        ["nested", "primitives"],
        { code: 7, name: "FromMixedArray" },
      ],
    },
  };

  const seen = [];
  const { dictionary, warnings } = flattenTaxonomy(ast, {
    onWarning: (message) => seen.push(message),
  });

  assert.deepEqual(Object.keys(dictionary).sort(), ["1044", "7"]);
  assert.equal(dictionary["1044"].name, "StillWorks");
  assert.equal(dictionary["7"].name, "FromMixedArray");

  assert.equal(warnings.length, 2);
  assert.match(
    warnings[0].message,
    /expected an error definition table but found an array/,
  );
  assert.match(
    warnings[1].message,
    /expected an error definition table but found a string/,
  );
  assert.equal(seen.length, 2);
});

test("flattenTaxonomy keeps the first definition and warns on duplicate codes", () => {
  const ast = {
    errors: [
      {
        code: 3,
        name: "AlreadyInitializedError",
        summary: "First definition wins.",
      },
      { code: 3, name: "ShadowedDuplicate", summary: "Must not overwrite." },
    ],
  };

  const { dictionary, warnings } = flattenTaxonomy(ast);

  assert.equal(dictionary["3"].name, "AlreadyInitializedError");
  assert.equal(warnings.length, 1);
  assert.match(warnings[0].message, /duplicate error code 3/);
});

test("flattenTaxonomy does not mistake descriptive sub-tables for definitions", () => {
  const ast = {
    errors: [
      {
        code: 0,
        name: "ContractError",
        common_causes: [
          { description: "Business logic failure", likelihood: "high" },
        ],
        suggested_fixes: [
          { description: "Use the resolver", difficulty: "easy" },
        ],
        related_errors: ["host.auth.not_authorized"],
      },
    ],
  };

  const { dictionary, warnings } = flattenTaxonomy(ast);

  assert.deepEqual(Object.keys(dictionary), ["0"]);
  assert.equal(dictionary["0"].common_causes.length, 1);
  assert.equal(warnings.length, 0);
});

test("buildTaxonomyJson produces a flat O(1) dictionary from the real contract.toml", async () => {
  const inputPath = path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "crates",
    "core",
    "src",
    "taxonomy",
    "data",
    "contract.toml",
  );
  const outputPath = path.join(
    os.tmpdir(),
    `taxonomy-test-${process.pid}.json`,
  );

  const { dictionary, warnings } = await buildTaxonomyJson({
    inputPath,
    outputPath,
  });

  assert.equal(warnings.length, 0);
  assert.equal(dictionary["0"].name, "ContractError");
  assert.equal(dictionary["6"].name, "AccountMissingError");

  for (const definition of Object.values(dictionary)) {
    assert.equal(typeof definition.name, "string");
    assert.equal(Array.isArray(definition), false);
  }
});
