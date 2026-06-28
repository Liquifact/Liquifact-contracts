'use strict';

/**
 * Health check service for dependency monitoring.
 * @module services/health
 */

const { getKycProviderConfig } = require('./kycService');
const { escrowIndexerLastCursorAdvanceTimestampSeconds, readinessGauge } = require('../metrics');
const db = require('../db/knex');
const cfg = require('../config');

/**
 * Classify Soroban RPC latency against configurable thresholds.
 * Returns "healthy" for latency <= warn, "degraded" for latency > warn && <= fail,
 * and "unhealthy" for latency > fail.
 * @param {number} latencyMs Measured latency in milliseconds.
 * @returns {'healthy'|'degraded'|'unhealthy'}
 */
function classifySorobanLatency(latencyMs) {
  const warnMs = parseInt(process.env.SOROBAN_LATENCY_WARN_MS, 10) || 200;
  const failMs = parseInt(process.env.SOROBAN_LATENCY_FAIL_MS, 10) || 500;
  if (latencyMs <= warnMs) return 'healthy';
  if (latencyMs <= failMs) return 'degraded';
  return 'unhealthy';
}

/**
 * Checks if the Soroban RPC endpoint is reachable and classifies latency.
 * @returns {Promise<{status: string, latency?: number, error?: string}>}
 */
async function checkSorobanHealth() {
  const url = process.env.SOROBAN_RPC_URL;
  if (!url) {
    return { status: 'unknown', error: 'SOROBAN_RPC_URL not configured' };
  }

  const start = Date.now();
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'getHealth' }),
      signal: controller.signal,
    });

    clearTimeout(timeout);
    const latency = Date.now() - start;

    if (response.ok) {
      const classification = classifySorobanLatency(latency);
      return { status: classification, latency };
    }
    return { status: 'unhealthy', latency, error: `HTTP ${response.status}` };
  } catch (error) {
    const latency = Date.now() - start;
    return { status: 'unhealthy', latency, error: error.message };
  }
}

/**
 * Checks if the database is reachable via a raw query.
 * Uses knex to run `SELECT 1` and measures latency.
 * Does not expose connection strings or hostnames in the response.
 * @returns {Promise<{status: string, latency?: number, error?: string}>}
 */
async function checkDatabaseHealth() {
  if (!process.env.DATABASE_URL) {
    return { status: 'not_configured' };
  }

  const pool = db.client && db.client.pool;
  if (pool) {
    const used = typeof pool.numUsed === 'function' ? pool.numUsed() : 0;
    const pending = typeof pool.numPendingAcquires === 'function' ? pool.numPendingAcquires() : 0;
    const max = pool.max || 10;

    if (used >= max || pending > 0) {
      const start = Date.now();
      try {
        await db.raw('SELECT 1');
        const latency = Date.now() - start;
        return { status: 'saturated', latency, error: 'Database connection pool exhausted' };
      } catch (_error) {
        const latency = Date.now() - start;
        return { status: 'unhealthy', latency, error: 'Database unreachable' };
      }
    }
  }

  const start = Date.now();
  try {
    await db.raw('SELECT 1');
    const latency = Date.now() - start;
    return { status: 'healthy', latency };
  } catch (_error) {
    const latency = Date.now() - start;
    return { status: 'unhealthy', latency, error: 'Database unreachable' };
  }
}

/**
 * Checks escrow reconciliation status.
 * 
 * @returns {Promise<{status: string, lastRun?: string, mismatches?: number, error?: string}>} Reconciliation health status.
 */
async function checkReconciliationHealth() {
  try {
    const { getReconciliationSummary } = require('../jobs/reconcileEscrow');
    const summary = await getReconciliationSummary();

    if (!summary) {
      return { status: 'not_run', error: 'Reconciliation has not been run yet' };
    }

    const lastRun = new Date(summary.reconciledAt);
    const hoursSinceLastRun = (Date.now() - lastRun.getTime()) / (1000 * 60 * 60);

    // Consider unhealthy if last run was more than 25 hours ago (allowing 1 hour grace)
    if (hoursSinceLastRun > 25) {
      return { status: 'stale', lastRun: summary.reconciledAt, error: 'Reconciliation not run recently' };
    }

    // Unhealthy if there are mismatches
    if (summary.mismatches > 0) {
      return { status: 'mismatches', lastRun: summary.reconciledAt, mismatches: summary.mismatches };
    }

    return { status: 'healthy', lastRun: summary.reconciledAt };
  } catch (error) {
    return { status: 'error', error: error.message };
  }
}

/**
 * Checks if the KYC provider is reachable.
 * Only runs when the provider is enabled (URL + API key configured).
 * The API key is sent in the Authorization header and never included in the response.
 * @returns {Promise<{status: string, latency?: number, error?: string}>}
 */
async function checkKycHealth() {
  const kycCfg = getKycProviderConfig();
  if (!kycCfg.enabled) {
    return { status: 'disabled' };
  }

  const start = Date.now();
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const response = await fetch(kycCfg.baseUrl, {
      method: 'HEAD',
      headers: { Authorization: `Bearer ${kycCfg.apiKey}` },
      signal: controller.signal,
    });

    clearTimeout(timeout);
    const latency = Date.now() - start;

    // Any HTTP response (even 4xx) means the host is reachable
    return response.ok || response.status < 500
      ? { status: 'healthy', latency }
      : { status: 'unhealthy', latency, error: `HTTP ${response.status}` };
  } catch (error) {
    const latency = Date.now() - start;
    return { status: 'unhealthy', latency, error: error.message };
  }
}

/**
 * Checks escrow indexer staleness.
 * Returns 'disabled' when the indexer is not enabled.
 * Returns 'stale' when the cursor hasn't advanced within the configured threshold.
 * Returns 'healthy' when the cursor has advanced recently or initially (gauge not yet set).
 *
 * @returns {Promise<{status: string, elapsedSeconds?: number, lastAdvanceTimestamp?: number, threshold?: number, error?: string}>} Indexer staleness health status.
 */
async function checkIndexerStaleness() {
  try {
    const config = cfg.get();

    // Check if indexer is enabled
    if (config.ESCROW_INDEXER_ENABLED !== 'true') {
      return { status: 'disabled' };
    }

    // Get the last advance timestamp from gauge
    const lastAdvanceTimestamp = escrowIndexerLastCursorAdvanceTimestampSeconds.get();

    // If gauge has never been set, treat as healthy (no false positive on startup)
    if (lastAdvanceTimestamp === undefined || lastAdvanceTimestamp === 0) {
      return { status: 'healthy', lastAdvanceTimestamp: 0, threshold: config.ESCROW_INDEXER_STALE_THRESHOLD_SECONDS };
    }

    const now = Math.floor(Date.now() / 1000);
    const elapsedSeconds = now - (lastAdvanceTimestamp || 0);
    const threshold = config.ESCROW_INDEXER_STALE_THRESHOLD_SECONDS || 300;

    if (elapsedSeconds > threshold) {
      return {
        status: 'stale',
        elapsedSeconds,
        lastAdvanceTimestamp,
        threshold,
        error: `Cursor has not advanced for ${elapsedSeconds} seconds (threshold: ${threshold})`,
      };
    }

    return {
      status: 'healthy',
      elapsedSeconds,
      lastAdvanceTimestamp,
      threshold,
    };
  } catch (error) {
    return { status: 'error', error: error.message };
  }
}

/**
 * Checks if the configured S3 bucket is reachable using the connectivity
 * probe defined by the storage service. The result is the raw probe output:
 * status plus optional latency and a sanitized error descriptor. Credentials,
 * endpoint URLs, and signed request headers are stripped before the result
 * is returned.
 *
 * Operator-visible statuses:
 *
 * - `'healthy'` — bucket is reachable and credentials authorize access.
 * - `'in_memory'` — in-memory fallback active; probe skipped.
 * - `'disabled'` — operator opted out via `S3_HEALTHCHECK_ENABLED=false`.
 * - `'not_configured'` — bucket name or credentials are missing.
 * - `'unhealthy'` — `HeadBucket` failed; `error.code` is an AWS error name.
 *
 * @returns {Promise<{
 *   status: string,
 *   latency?: number,
 *   error?: {code: string, hint: string},
 *   bucketConfigured?: boolean,
 *   credentialsConfigured?: boolean
 * }>}
 */
async function checkStorageHealth() {
  const storage = require('./storage');
  return storage.probeS3Connectivity();
}

/**
 * Performs all dependency health checks.
 * @returns {Promise<{healthy: boolean, checks: Object}>}
 */
async function performHealthChecks() {
  const [soroban, database, kyc, indexerStaleness, storage] = await Promise.all([
    checkSorobanHealth(),
    checkDatabaseHealth(),
    checkKycHealth(),
    checkIndexerStaleness(),
    checkStorageHealth(),
  ]);

  const checks = { soroban, database, kyc, indexerStaleness, storage };
  const healthy =
    (soroban.status === 'healthy' || soroban.status === 'unknown') &&
    (kyc.status === 'healthy' || kyc.status === 'disabled') &&
    (indexerStaleness.status === 'healthy' || indexerStaleness.status === 'disabled') &&
    // Storage is in-memory or opted out → non-blocking. Missing config or
    // unreachable buckets → blocking for `/ready` because uploads will fail.
    storage.status !== 'not_configured' &&
    storage.status !== 'unhealthy';

  return { healthy, checks };
}

/**
 * Performs critical-dependency readiness checks (DB, Soroban RPC, storage).
 * The KYC and indexer staleness checks are omitted because they are not
 * required for the process to serve *most* traffic — only critical
 * upstream dependencies that would prevent any business request from
 * completing are included.
 *
 * The S3 storage probe is included so a misconfigured bucket (wrong
 * endpoint, bad credentials, deleted bucket) is surfaced on the readiness
 * probe rather than at the first invoice upload.
 *
 * Updates the `readiness_gauge` Prometheus metric (1 = ready, 0 = not
 * ready). Degraded Soroban RPC (slow) does NOT block readiness.
 *
 * @returns {Promise<{
 *   healthy: boolean,
 *   checks: {
 *     database: Object,
 *     soroban: Object,
 *     storage: Object
 *   }
 * }>}
 */
async function performReadinessChecks() {
  const [database, soroban, storage] = await Promise.all([
    checkDatabaseHealth(),
    checkSorobanHealth(),
    checkStorageHealth(),
  ]);

  const checks = { database, soroban, storage };
  // In-memory and explicitly-disabled probes are treated as readiness-OK.
  // Production deployments missing the bucket or with an unreachable
  // bucket DO block readiness.
  const storageOk =
    storage.status === 'healthy' ||
    storage.status === 'in_memory' ||
    storage.status === 'disabled';

  // Determine overall readiness. Degraded Soroban RPC (slow) does NOT block readiness.
  const healthy =
    database.status === 'healthy' &&
    (soroban.status === 'healthy' || soroban.status === 'degraded' || soroban.status === 'unknown') &&
    storageOk;

  // Set gauge: 1 = ready, 0.5 = degraded, 0 = not ready
  if (database.status !== 'healthy' || !storageOk) {
    readinessGauge.set(0);
  } else if (soroban.status === 'degraded') {
    readinessGauge.set(0.5);
  } else {
    readinessGauge.set(healthy ? 1 : 0);
  }

  return { healthy, checks };
}

module.exports = {
  checkSorobanHealth,
  checkDatabaseHealth,
  checkKycHealth,
  checkStorageHealth,
  checkIndexerStaleness,
  performHealthChecks,
  performReadinessChecks,
};
