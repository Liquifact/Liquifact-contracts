'use strict';

/**
 * @fileoverview Opaque cursor encoding/decoding for marketplace keyset pagination.
 *
 * Cursors are base64url-encoded JSON objects containing the last-seen value for
 * the active sort field and the row `id` tiebreaker. They are HMAC-signed so
 * that any modification — or reuse with a different sort field — is detected
 * and rejected with a 400 before it reaches the database layer.
 *
 * Opaque cursor payload shape:
 * ```json
 * { "sortField": "yield_bps", "sortValue": 450, "id": "inv_abc123", "iat": 1719187200 }
 * ```
 *
 * @module utils/cursorPagination
 */

const crypto = require('crypto');

/**
 * Secret used to sign cursors.
 * @type {string}
 */
const CURSOR_SECRET = process.env.CURSOR_SECRET || process.env.JWT_SECRET || 'dev-cursor-secret-change-in-prod';

/**
 * Allowed sort fields for marketplace queries.
 * @type {ReadonlyArray<string>}
 */
const ALLOWED_SORT_FIELDS = Object.freeze(['yield_bps', 'maturity_date', 'funded_ratio', 'amount', 'created_at']);

/**
 * Computes HMAC-SHA-256 digest over payload.
 *
 * @param {string} payload - The string to sign.
 * @returns {string} Hex-encoded HMAC digest.
 */
function _sign(payload) {
  return crypto.createHmac('sha256', CURSOR_SECRET).update(payload).digest('hex');
}

/**
 * Encodes a cursor from the last row returned in a page.
 *
 * @param {Object} params
 * @param {string} params.sortField - The active sort column.
 * @param {*}      params.sortValue - The sort-column value from the last row.
 * @param {string} params.id        - The `id` of the last row.
 * @returns {string} Opaque base64url cursor string.
 */
function encodeCursor({ sortField, sortValue, id }) {
  if (!ALLOWED_SORT_FIELDS.includes(sortField)) {
    throw new Error(`encodeCursor: unsupported sortField "${sortField}"`);
  }
  if (!id || typeof id !== 'string') {
    throw new Error('encodeCursor: id must be a non-empty string');
  }

  const payload = JSON.stringify({
    sortField,
    sortValue,
    id,
    iat: Math.floor(Date.now() / 1000),
  });

  const b64 = Buffer.from(payload).toString('base64url');
  const sig = _sign(b64);
  return `${b64}.${sig}`;
}

/**
 * Decodes and validates an opaque cursor string.
 *
 * @param {string} cursor            - The opaque cursor from the client.
 * @param {string} expectedSortField - The sort field in the current request.
 * @returns {{ sortField: string, sortValue: *, id: string, iat: number }}
 * @throws {CursorError} When the cursor is malformed, tampered, or mismatched.
 */
function decodeCursor(cursor, expectedSortField) {
  if (typeof cursor !== 'string' || !cursor.includes('.')) {
    throw new CursorError('Malformed cursor: expected base64url.signature format');
  }

  const dotIdx = cursor.lastIndexOf('.');
  const b64 = cursor.slice(0, dotIdx);
  const sig = cursor.slice(dotIdx + 1);

  const expectedSig = _sign(b64);
  const sigBuf = Buffer.from(sig, 'hex');
  const expectedBuf = Buffer.from(expectedSig, 'hex');

  if (
    sigBuf.length !== expectedBuf.length ||
    !crypto.timingSafeEqual(sigBuf, expectedBuf)
  ) {
    throw new CursorError('Invalid cursor signature');
  }

  let parsed;
  try {
    parsed = JSON.parse(Buffer.from(b64, 'base64url').toString('utf8'));
  } catch {
    throw new CursorError('Malformed cursor: payload is not valid JSON');
  }

  const { sortField, sortValue, id, iat } = parsed;

  if (!ALLOWED_SORT_FIELDS.includes(sortField)) {
    throw new CursorError(`Cursor contains unknown sort field "${sortField}"`);
  }
  if (typeof id !== 'string' || id.length === 0) {
    throw new CursorError('Cursor is missing a valid id tiebreaker');
  }
  if (typeof iat !== 'number') {
    throw new CursorError('Cursor is missing issued-at timestamp');
  }

  if (process.env.CURSOR_TTL_ENABLED === 'true') {
    const ttl = parseInt(process.env.CURSOR_TTL_SECONDS || '3600', 10);
    const now = Math.floor(Date.now() / 1000);
    if (iat && (now - iat) > ttl) {
      throw new CursorError('Cursor has expired');
    }
  }

  if (sortField !== expectedSortField) {
    throw new CursorError(
      `Cursor sort field "${sortField}" does not match requested sort field "${expectedSortField}"`
    );
  }

  return { sortField, sortValue, id, iat };
}

/**
 * Domain error for cursor-related failures.
 */
class CursorError extends Error {
  /**
   * Creates an instance of CursorError.
   * @param {string} message The error message.
   */
  constructor(message) {
    super(message);
    this.name = 'CursorError';
  }
}

module.exports = {
  encodeCursor,
  decodeCursor,
  CursorError,
  ALLOWED_SORT_FIELDS,
};
