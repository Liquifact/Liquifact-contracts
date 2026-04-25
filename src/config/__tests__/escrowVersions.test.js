"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const MODULE_PATH = path.resolve(__dirname, "..", "escrowVersions.js");

// ---------------------------------------------------------------------------
// getExpectedSchemaVersion — loaded with a valid env var
// ---------------------------------------------------------------------------

describe("getExpectedSchemaVersion", () => {
  let getExpectedSchemaVersion;

  beforeEach(() => {
    process.env.ESCROW_CONTRACT_SEMVER = "0.1.0";
    // Clear require cache so the IIFE re-runs with the current env.
    delete require.cache[MODULE_PATH];
    ({ getExpectedSchemaVersion } = require(MODULE_PATH));
  });

  afterEach(() => {
    delete process.env.ESCROW_CONTRACT_SEMVER;
    delete require.cache[MODULE_PATH];
  });

  it('returns 5 for "0.1.0"', () => {
    assert.strictEqual(getExpectedSchemaVersion("0.1.0"), 5);
  });

  it('throws containing "unknown semver" for an unregistered version', () => {
    assert.throws(
      () => getExpectedSchemaVersion("9.9.9"),
      (err) => {
        assert.ok(err.message.includes("unknown semver"), `unexpected message: ${err.message}`);
        return true;
      }
    );
  });
});

// ---------------------------------------------------------------------------
// CURRENT_SEMVER — missing env var must throw at load time
// ---------------------------------------------------------------------------

describe("CURRENT_SEMVER", () => {
  beforeEach(() => {
    delete process.env.ESCROW_CONTRACT_SEMVER;
    delete require.cache[MODULE_PATH];
  });

  afterEach(() => {
    delete require.cache[MODULE_PATH];
  });

  it("throws at load time when ESCROW_CONTRACT_SEMVER is not set", () => {
    assert.throws(
      () => require(MODULE_PATH),
      (err) => {
        assert.ok(
          err.message.includes("ESCROW_CONTRACT_SEMVER"),
          `unexpected message: ${err.message}`
        );
        return true;
      }
    );
  });

  it("exposes the env value as CURRENT_SEMVER when set", () => {
    process.env.ESCROW_CONTRACT_SEMVER = "0.1.0";
    const { CURRENT_SEMVER } = require(MODULE_PATH);
    assert.strictEqual(CURRENT_SEMVER, "0.1.0");
  });
});
