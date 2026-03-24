/**
 * OpenAPI schema conformance tests for LiquiFact API.
 *
 * Validates that request/response payloads conform to the schemas defined in
 * docs/openapi.yaml using AJV (JSON Schema draft-07 / OpenAPI 3.1 subset).
 *
 * Run:  npm test  (from the docs/ directory)
 */

"use strict";

const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const yaml = require("js-yaml");
const Ajv = require("ajv");
const addFormats = require("ajv-formats");

// ---------------------------------------------------------------------------
// Load spec and build validators
// ---------------------------------------------------------------------------

const specPath = path.join(__dirname, "..", "openapi.yaml");
const spec = yaml.load(fs.readFileSync(specPath, "utf8"));
const schemas = spec.components.schemas;

const ajv = new Ajv({ allErrors: true, strict: false });
addFormats(ajv);

// Register every component schema so $ref resolution works.
for (const [name, schema] of Object.entries(schemas)) {
  ajv.addSchema(schema, `#/components/schemas/${name}`);
}

/**
 * Compile a schema reference and return a validate function.
 * @param {string} ref - e.g. "#/components/schemas/InvoiceEscrow"
 */
function validator(ref) {
  return ajv.compile({ $ref: ref });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Assert that `data` is valid against `schemaRef`.
 * @param {string} schemaRef
 * @param {unknown} data
 */
function assertValid(schemaRef, data) {
  const validate = validator(schemaRef);
  const ok = validate(data);
  assert.ok(ok, `Expected valid ${schemaRef} but got errors: ${JSON.stringify(validate.errors)}`);
}

/**
 * Assert that `data` is INVALID against `schemaRef`.
 * @param {string} schemaRef
 * @param {unknown} data
 */
function assertInvalid(schemaRef, data) {
  const validate = validator(schemaRef);
  const ok = validate(data);
  assert.ok(!ok, `Expected invalid ${schemaRef} but validation passed for: ${JSON.stringify(data)}`);
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// 56-char Stellar G-address (base32 A-Z2-7)
const VALID_ADDRESS = "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5";

const validEscrow = {
  invoice_id: "INV-1023",
  sme_address: VALID_ADDRESS,
  amount: 100_000_000_000,
  funding_target: 100_000_000_000,
  funded_amount: 0,
  yield_bps: 800,
  maturity: 1000,
  status: 0,
};

// ---------------------------------------------------------------------------
// StellarAddress
// ---------------------------------------------------------------------------

describe("StellarAddress schema", () => {
  const ref = "#/components/schemas/StellarAddress";

  it("accepts a valid 56-char base32 address", () => {
    assertValid(ref, VALID_ADDRESS);
  });

  it("rejects an address that is too short", () => {
    assertInvalid(ref, "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOC");
  });

  it("rejects an address with lowercase letters", () => {
    assertInvalid(ref, "gaazi4tcr3ty5ojhctjc2a4qsy6cjwjh5iajtgkin2er7lbnvkoccwn");
  });

  it("rejects an empty string", () => {
    assertInvalid(ref, "");
  });
});

// ---------------------------------------------------------------------------
// InvoiceId
// ---------------------------------------------------------------------------

describe("InvoiceId schema", () => {
  const ref = "#/components/schemas/InvoiceId";

  it("accepts a typical invoice id", () => {
    assertValid(ref, "INV-1023");
  });

  it("accepts alphanumeric with underscores and hyphens", () => {
    assertValid(ref, "INV_001-A");
  });

  it("rejects an empty string", () => {
    assertInvalid(ref, "");
  });

  it("rejects a string longer than 12 chars", () => {
    assertInvalid(ref, "INVOICE-99999");
  });

  it("rejects special characters", () => {
    assertInvalid(ref, "INV#1023");
  });
});

// ---------------------------------------------------------------------------
// EscrowStatus
// ---------------------------------------------------------------------------

describe("EscrowStatus schema", () => {
  const ref = "#/components/schemas/EscrowStatus";

  it("accepts 0 (open)", () => assertValid(ref, 0));
  it("accepts 1 (funded)", () => assertValid(ref, 1));
  it("accepts 2 (settled)", () => assertValid(ref, 2));
  it("rejects 3 (unknown)", () => assertInvalid(ref, 3));
  it("rejects -1", () => assertInvalid(ref, -1));
  it("rejects a string", () => assertInvalid(ref, "open"));
});

// ---------------------------------------------------------------------------
// InvoiceEscrow
// ---------------------------------------------------------------------------

describe("InvoiceEscrow schema", () => {
  const ref = "#/components/schemas/InvoiceEscrow";

  it("accepts a fully-funded escrow", () => {
    assertValid(ref, { ...validEscrow, funded_amount: 100_000_000_000, status: 1 });
  });

  it("accepts a settled escrow", () => {
    assertValid(ref, { ...validEscrow, funded_amount: 100_000_000_000, status: 2 });
  });

  it("accepts an open escrow with zero funded_amount", () => {
    assertValid(ref, validEscrow);
  });

  it("rejects missing invoice_id", () => {
    const { invoice_id, ...rest } = validEscrow;
    assertInvalid(ref, rest);
  });

  it("rejects missing sme_address", () => {
    const { sme_address, ...rest } = validEscrow;
    assertInvalid(ref, rest);
  });

  it("rejects amount = 0", () => {
    assertInvalid(ref, { ...validEscrow, amount: 0 });
  });

  it("rejects negative funded_amount", () => {
    assertInvalid(ref, { ...validEscrow, funded_amount: -1 });
  });

  it("rejects yield_bps > 10000", () => {
    assertInvalid(ref, { ...validEscrow, yield_bps: 10001 });
  });

  it("rejects yield_bps < 0", () => {
    assertInvalid(ref, { ...validEscrow, yield_bps: -1 });
  });

  it("rejects invalid status value", () => {
    assertInvalid(ref, { ...validEscrow, status: 5 });
  });

  it("rejects invalid sme_address format", () => {
    assertInvalid(ref, { ...validEscrow, sme_address: "not-an-address" });
  });
});

// ---------------------------------------------------------------------------
// InvoiceSummary
// ---------------------------------------------------------------------------

describe("InvoiceSummary schema", () => {
  const ref = "#/components/schemas/InvoiceSummary";

  it("accepts a minimal valid summary", () => {
    assertValid(ref, { invoice_id: "INV001", amount: 1, status: 0 });
  });

  it("rejects missing amount", () => {
    assertInvalid(ref, { invoice_id: "INV001", status: 0 });
  });

  it("rejects amount = 0", () => {
    assertInvalid(ref, { invoice_id: "INV001", amount: 0, status: 0 });
  });
});

// ---------------------------------------------------------------------------
// InitEscrowRequest
// ---------------------------------------------------------------------------

describe("InitEscrowRequest schema", () => {
  const ref = "#/components/schemas/InitEscrowRequest";

  const valid = {
    invoice_id: "INV-1023",
    sme_address: VALID_ADDRESS,
    amount: 100_000_000_000,
    yield_bps: 800,
    maturity: 1000,
  };

  it("accepts a valid request", () => assertValid(ref, valid));

  it("rejects missing sme_address", () => {
    const { sme_address, ...rest } = valid;
    assertInvalid(ref, rest);
  });

  it("rejects amount = 0", () => {
    assertInvalid(ref, { ...valid, amount: 0 });
  });

  it("rejects maturity = 0", () => {
    assertInvalid(ref, { ...valid, maturity: 0 });
  });

  it("rejects yield_bps > 10000", () => {
    assertInvalid(ref, { ...valid, yield_bps: 10001 });
  });

  it("rejects invoice_id longer than 12 chars", () => {
    assertInvalid(ref, { ...valid, invoice_id: "TOOLONGINVOICEID" });
  });
});

// ---------------------------------------------------------------------------
// FundEscrowRequest
// ---------------------------------------------------------------------------

describe("FundEscrowRequest schema", () => {
  const ref = "#/components/schemas/FundEscrowRequest";

  const valid = { investor_address: VALID_ADDRESS, amount: 50_000_000_000 };

  it("accepts a valid fund request", () => assertValid(ref, valid));

  it("rejects missing investor_address", () => {
    assertInvalid(ref, { amount: 50_000_000_000 });
  });

  it("rejects amount = 0", () => {
    assertInvalid(ref, { ...valid, amount: 0 });
  });

  it("rejects negative amount", () => {
    assertInvalid(ref, { ...valid, amount: -1 });
  });

  it("rejects invalid investor_address", () => {
    assertInvalid(ref, { ...valid, investor_address: "bad" });
  });
});

// ---------------------------------------------------------------------------
// HealthResponse
// ---------------------------------------------------------------------------

describe("HealthResponse schema", () => {
  const ref = "#/components/schemas/HealthResponse";

  it("accepts a valid health response", () => {
    assertValid(ref, { status: "ok", timestamp: "2026-03-23T21:46:03Z" });
  });

  it("rejects status other than 'ok'", () => {
    assertInvalid(ref, { status: "degraded", timestamp: "2026-03-23T21:46:03Z" });
  });

  it("rejects missing timestamp", () => {
    assertInvalid(ref, { status: "ok" });
  });

  it("rejects invalid timestamp format", () => {
    assertInvalid(ref, { status: "ok", timestamp: "not-a-date" });
  });
});

// ---------------------------------------------------------------------------
// ApiInfoResponse
// ---------------------------------------------------------------------------

describe("ApiInfoResponse schema", () => {
  const ref = "#/components/schemas/ApiInfoResponse";

  it("accepts a valid info response", () => {
    assertValid(ref, { name: "LiquiFact API", version: "1.0.0", network: "mainnet" });
  });

  it("accepts optional docs_url", () => {
    assertValid(ref, {
      name: "LiquiFact API",
      version: "1.0.0",
      network: "testnet",
      docs_url: "https://docs.liquifact.io",
    });
  });

  it("rejects unknown network value", () => {
    assertInvalid(ref, { name: "LiquiFact API", version: "1.0.0", network: "devnet" });
  });

  it("rejects missing name", () => {
    assertInvalid(ref, { version: "1.0.0", network: "mainnet" });
  });
});

// ---------------------------------------------------------------------------
// ErrorResponse
// ---------------------------------------------------------------------------

describe("ErrorResponse schema", () => {
  const ref = "#/components/schemas/ErrorResponse";

  it("accepts a valid error response", () => {
    assertValid(ref, { error: "NOT_FOUND", message: "Invoice not found." });
  });

  it("rejects missing error field", () => {
    assertInvalid(ref, { message: "Invoice not found." });
  });

  it("rejects missing message field", () => {
    assertInvalid(ref, { error: "NOT_FOUND" });
  });
});
