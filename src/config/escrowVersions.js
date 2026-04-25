"use strict";

/**
 * Maps contract semver → on-chain SCHEMA_VERSION (DataKey::Version).
 * Update this map each time a new WASM is deployed with a bumped SCHEMA_VERSION.
 * Current on-chain value: SCHEMA_VERSION = 5 (escrow/src/lib.rs).
 */
const ESCROW_VERSIONS = Object.freeze({
  "0.1.0": 5,
});

/**
 * Return the expected on-chain SCHEMA_VERSION for a given semver string.
 * @param {string} semver - e.g. "0.1.0"
 * @returns {number}
 * @throws {Error} if semver is not in ESCROW_VERSIONS
 */
function getExpectedSchemaVersion(semver) {
  if (!Object.prototype.hasOwnProperty.call(ESCROW_VERSIONS, semver)) {
    throw new Error(`unknown semver "${semver}" — add it to ESCROW_VERSIONS`);
  }
  return ESCROW_VERSIONS[semver];
}

/**
 * Semver of the currently deployed WASM, read from env at startup.
 * Set ESCROW_CONTRACT_SEMVER in your deployment environment (never in source).
 */
const CURRENT_SEMVER = (() => {
  const v = process.env.ESCROW_CONTRACT_SEMVER;
  if (!v) {
    throw new Error("ESCROW_CONTRACT_SEMVER env var is required but not set");
  }
  return v;
})();

module.exports = { ESCROW_VERSIONS, getExpectedSchemaVersion, CURRENT_SEMVER };
