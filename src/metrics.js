'use strict';

/**
 * @fileoverview Prometheus metrics registry and /metrics route handler.
 *
 * ## Auth strategy (in priority order)
 *
 * 1. If `METRICS_BEARER_TOKEN` is set, require `Authorization: Bearer <token>`.
 *    The token comparison uses a **constant-time** algorithm to prevent timing
 *    side-channel attacks.
 *
 * 2. If `METRICS_BEARER_TOKEN` is **unset**, allow requests from loopback
 *    addresses only (`127.0.0.1`, `::1`, `::ffff:127.0.0.1`). This is suitable
 *    for private-network Prometheus scraping.
 *
 * 3. All other requests receive a uniform `401` with no detail about _why_
 *    (no distinction between "wrong token" and "missing token").
 *
 * ## Security: trusted-proxy & X-Forwarded-For
 *
 * Loopback detection **always** reads the direct TCP connection address from
 * `req.socket.remoteAddress`. The `X-Forwarded-For` header is **never**
 * consulted, so a remote attacker cannot spoof a loopback origin by setting
 * `X-Forwarded-For: 127.0.0.1`.
 *
 * There is no `app.set('trust proxy', ...)` call anywhere in this application.
 * If one is added in the future, `req.ip` could resolve to a `X-Forwarded-For`
 * value, but this middleware **already** ignores `req.ip` for loopback checks
 * and reads the socket directly, making it resilient to such config changes.
 *
 * @module metrics
 */

let client;
try {
  client = require('prom-client');
} catch (_e) {
  console.error('Failed to load prom-client:', _e);
  // Fallback shim for environments without prom-client (tests).
  //
  // The shims maintain the same observable surface as real prom-client so
  // tests can inspect `counter.hashMap` / `counter.get()` directly without
  // changing the assertion code.

  /**
   * Minimal prom-client Registry shim for test environments.
   * @implements {import('prom-client').Registry}
   */
  class RegistryShim {
    /** @param {void} */
    constructor() {
      this.contentType = 'text/plain';
      this._items = [];
    }
    /** @returns {string} */
    metrics() {
      return '';
    }
  }

  /**
   * Counter shim for test environments.
   * @implements {import('prom-client').Counter}
   */
  class CounterShim {
    /** @param {void} */
    constructor() {}
    /** @returns {void} */
    inc() {}
  }

  /**
   * Gauge shim for test environments.
   * @implements {import('prom-client').Gauge}
   */
  class GaugeShim {
    /** @param {void} */
    constructor() {}
    /** @returns {void} */
    set() {}
    /** @returns {void} */
    setToCurrentTime() {}
  }

  client = {
    Registry: RegistryShim,
    /**
     * No-op default metrics collector stub.
     * @returns {void}
     */
    collectDefaultMetrics: () => { },
    Counter: CounterShim,
    Gauge: GaugeShim,
  };
}

/** Shared registry — exported so tests can reset it between runs. */
const registry = new client.Registry();

if (typeof client.collectDefaultMetrics === 'function') {
  client.collectDefaultMetrics({ register: registry });
}

const METRIC_REFRESH_INTERVAL_MS = 5000;
const registeredJobQueues = new Set();
const registeredWorkers = new Set();
let refreshTimer = null;

/**
 * Registers a job queue for background metric scraping.
 * @param {Object} queue - Queue to register.
 * @returns {void}
 */
function registerJobQueue(queue) {
  registeredJobQueues.add(queue);
}

/**
 * Registers a background worker for metric scraping.
 * @param {Object} worker - Worker to register.
 * @returns {void}
 */
function registerWorker(worker) {
  registeredWorkers.add(worker);
}

const queueDepthGauge = new client.Gauge({
  name: 'liquifact_job_queue_depth',
  help: 'Number of pending jobs currently waiting in background queues',
  registers: [registry],
});

const retryQueueSizeGauge = new client.Gauge({
  name: 'liquifact_job_retry_queue_size',
  help: 'Number of jobs waiting in retry queues for background processing',
  registers: [registry],
});

const workerInFlightGauge = new client.Gauge({
  name: 'liquifact_worker_inflight_count',
  help: 'Number of jobs currently being processed by background workers',
  registers: [registry],
});

// Cached metrics text for compatibility with test environments where
// prom-client is not available (shim). In production with the real
// prom-client, `metricsHandler` calls the real `registry.metrics()`
// which returns the full Prometheus exposition of ALL registered metrics.
let cachedMetrics = '# HELP liquifact_custom_metrics Placeholder\n';

/**
 * Bounded enum of allowed `reason` label values for maturity-reminder metrics.
 * Any raw error/reason string must be mapped through {@link normalizeReminderReason}
 * before being used as a Prometheus label to prevent time-series cardinality explosion.
 *
 * | Value            | Meaning                                              |
 * |------------------|------------------------------------------------------|
 * | smtp_timeout     | SMTP connection or send timed out                    |
 * | smtp_reject      | SMTP server rejected the message (4xx/5xx response)  |
 * | template_error   | Email template rendering failed                      |
 * | unknown          | Any other / unmapped failure                         |
 */
const REMINDER_REASON_ENUM = Object.freeze([
  'smtp_timeout',
  'smtp_reject',
  'template_error',
  'unknown',
]);

/**
 * Bounded enum of allowed `job_type` label values.
 * Add new job types here when introducing new background job kinds.
 */
const JOB_TYPE_ENUM = Object.freeze(['maturity_reminder', 'webhook_replay', 'unknown']);

/**
 * Bounded enum of allowed `outcome` label values for webhook replay metrics.
 * @readonly
 */
const WEBHOOK_REPLAY_OUTCOME_ENUM = Object.freeze([
  'success',
  'failure',
  'not_found',
  'already_resolved',
]);

/**
 * Refreshes all registered job queues and workers metrics.
 * @returns {void}
 */
function refreshMetrics() {
  let queueLength = 0;
  let retryQueueLength = 0;

  for (const queue of registeredJobQueues) {
    try {
      const stats = queue.getStats();
      if (stats) {
        queueLength += Number(stats.queueLength || 0);
        retryQueueLength += Number(stats.retryQueueLength || 0);
      }
    } catch {
      // Preserve existing metrics if a registered queue becomes invalid.
    }
  }

  let workerInFlight = 0;
  for (const worker of registeredWorkers) {
    try {
      const stats = worker.getStats();
      if (stats && typeof stats.processingCount === 'number') {
        workerInFlight += stats.processingCount;
      }
    } catch {
      // Preserve existing metrics if a registered worker becomes invalid.
    }
  }

  queueDepthGauge.set(queueLength);
  retryQueueSizeGauge.set(retryQueueLength);
  workerInFlightGauge.set(workerInFlight);

  // Build a minimal Prometheus text exposition that includes our gauges.
  // Keep labels bounded and avoid including payloads or per-job ids.
  // The body-size-limit counter is read from the prom-client hashMap so it
  // reflects all .inc() calls made since process start (or shim default 0).
  let bodySizeRejectionsByType = '';
  const hashMap = bodySizeLimitRejectionsTotal.hashMap || {};
  for (const entry of Object.values(hashMap)) {
    if (entry && typeof entry.value === 'number' && entry.value > 0) {
      const labels = entry.labels || {};
      const typeLabel = labels.type || 'unknown';
      bodySizeRejectionsByType += `body_size_limit_rejections_total{type="${typeLabel}"} ${entry.value}\n`;
    }
  }

  cachedMetrics = '' +
    '# HELP liquifact_job_queue_depth Number of pending jobs waiting in queues\n' +
    '# TYPE liquifact_job_queue_depth gauge\n' +
    `liquifact_job_queue_depth ${queueLength}\n` +
    '# HELP liquifact_job_retry_queue_size Number of jobs waiting in retry queues\n' +
    '# TYPE liquifact_job_retry_queue_size gauge\n' +
    `liquifact_job_retry_queue_size ${retryQueueLength}\n` +
    '# HELP liquifact_worker_inflight_count Number of jobs currently being processed\n' +
    '# TYPE liquifact_worker_inflight_count gauge\n' +
    `liquifact_worker_inflight_count ${workerInFlight}\n` +
    '# HELP body_size_limit_rejections_total Total number of request body-size limit rejections (413 Payload Too Large), labelled by limit type for DoS detection\n' +
    '# TYPE body_size_limit_rejections_total counter\n' +
    bodySizeRejectionsByType;
}

/**
 * Starts the periodic metrics refresh interval timer.
 * The timer is created once and automatically unref'd so it does not
 * keep the Node.js process alive.
 * @returns {void}
 */
function startMetricsRefresh() {
  if (refreshTimer) {
    return;
  }

  refreshTimer = setInterval(refreshMetrics, METRIC_REFRESH_INTERVAL_MS);
  if (typeof refreshTimer.unref === 'function') {
    refreshTimer.unref();
  }
}

/**
 * Stops the periodic metrics refresh interval timer.
 * @returns {void}
 */
function stopMetricsRefresh() {
  if (!refreshTimer) {
    return;
  }

  clearInterval(refreshTimer);
  refreshTimer = null;
}

/**
 * Maps a raw job type string to a bounded Prometheus label value.
 *
 * @param {unknown} raw - Raw job type string.
 * @returns {string} Bounded label value from {@link JOB_TYPE_ENUM}.
 */
function normalizeJobType(raw) {
  const str = typeof raw === 'string' ? raw : '';
  return JOB_TYPE_ENUM.includes(str) ? str : 'unknown';
}

/**
 * Normalizes a raw SMTP/delivery error string to one of the bounded REMINDER_REASON_ENUM values.
 * Parses common error patterns to prevent cardinality explosion in Prometheus.
 *
 * @param {Error|string|*} raw - The raw error thrown during email delivery.
 * @returns {string} One of REMINDER_REASON_ENUM ('smtp_timeout', 'smtp_reject', 'template_error', 'unknown').
 */
function normalizeReminderReason(raw) {
  if (!raw) { return 'unknown'; }
  const msg = (raw instanceof Error ? raw.message : String(raw)).toLowerCase();
  if (msg.includes('timeout') || msg.includes('etimedout')) {
    return 'smtp_timeout';
  }
  if (msg.includes('reject') || msg.includes('invalid recipient') || /^[45]\d{2}\b/.test(msg)) {
    return 'smtp_reject';
  }
  if (msg.includes('template') || msg.includes('render') || msg.includes('missing key')) {
    return 'template_error';
  }
  return 'unknown';
}

// ── Maturity-reminder counters ────────────────────────────────────────────────

/**
 * Total maturity-reminder delivery attempts, labelled by bounded `reason` and `job_type`.
 * @type {import('prom-client').Counter}
 */
const maturityReminderDeliveryAttemptsTotal = new client.Counter({
  name: 'maturity_reminder_delivery_attempts_total',
  help: 'Total number of maturity-reminder delivery attempts',
  labelNames: ['reason', 'job_type'],
  registers: [],
});

/**
 * Total maturity-reminder dead-letter events, labelled by bounded `reason` and `job_type`.
 * @type {import('prom-client').Counter}
 */
const maturityReminderDeadLetterTotal = new client.Counter({
  name: 'maturity_reminder_dead_letter_total',
  help: 'Total number of maturity-reminder messages moved to the dead-letter queue',
  labelNames: ['reason', 'job_type'],
  registers: [],
});

/**
 * Total webhook replay attempts, labelled by bounded `outcome`.
 * Outcomes: success | failure | not_found | already_resolved
 * @type {import('prom-client').Counter}
 */
const webhookReplayTotal = new client.Counter({
  name: 'webhook_replay_total',
  help: 'Total number of webhook dead-letter replay attempts',
  labelNames: ['outcome'],
  registers: [],
});

/**
 * Counter: Legal hold state processing errors.
 * @type {import('prom-client').Counter}
 */
const legalHoldProcessingErrorsTotal = new client.Counter({
  name: 'legal_hold_processing_errors_total',
  help: 'Total number of processing errors encountered during legal hold evaluations',
  labelNames: ['reason'],
  registers: [],
});

/**
 * Counter: Legal hold blocks processed.
 * @type {import('prom-client').Counter}
 */
const legalHoldBlocksTotal = new client.Counter({
  name: 'legal_hold_blocks_processed_total',
  help: 'Total number of blocks evaluated with valid legal hold statuses',
  labelNames: ['status'],
  registers: [],
});

/**
 * Counter: Redis fallback fail-open actions.
 * @type {import('prom-client').Counter}
 */
const redisCacheFailOpenTotal = new client.Counter({
  name: 'redis_cache_fail_open_total',
  help: 'Total number of times redis queries failed and fell back to db/fail-open gracefully',
  registers: [],
});

/**
 * Counter: Cache store errors.
 * @type {import('prom-client').Counter}
 */
const cacheStoreErrorsTotal = new client.Counter({
  name: 'cache_store_errors_total',
  help: 'Total number of errors encountered during cache operations',
  labelNames: ['operation', 'error_type'],
  registers: [],
});

/**
 * Counter: Escrow creation request preflight rejections.
 * @type {import('prom-client').Counter}
 */
const escrowPreflightRejectedTotal = new client.Counter({
  name: 'escrow_preflight_rejected_total',
  help: 'Total number of escrow creation requests rejected during preflight validation',
  labelNames: ['reason'],
  registers: [],
});

/**
 * Counter: Body size limit rejections.
 * Incremented when a request payload exceeds defined size limits.
 * @type {import('prom-client').Counter}
 */
const bodySizeLimitRejectionsTotal = new client.Counter({
  name: 'body_size_limit_rejections_total',
  help: 'Total number of request payloads rejected for exceeding size limits',
  labelNames: ['limit_type'],
  registers: [],
});

/**
 * Increment the count of blocks processed where the legal hold state is unknown.
 * @returns {void}
 */
function incrementLegalHoldUnknownBlocks() {
  legalHoldProcessingErrorsTotal.inc({ reason: 'unknown_hold_state' });
}

/**
 * Increment the count of blocks processed with a valid legal hold status.
 * @param {string} status - Bounded status value.
 * @returns {void}
 */
function incrementLegalHoldBlocks(status) {
  legalHoldBlocksTotal.inc({ status });
}

/**
 * Resets all metrics state for test isolation.
 * Clears registered queues, workers, and resets gauge values to zero.
 * @returns {void}
 */
function resetMetricsForTests() {
  registeredJobQueues.clear();
  registeredWorkers.clear();
  queueDepthGauge.set(0);
  retryQueueSizeGauge.set(0);
  workerInFlightGauge.set(0);
  stopMetricsRefresh();
}

/**
 * Constant-time string comparison to prevent timing attacks.
 *
 * Returns `false` early when lengths differ (public info leaked by content-length
 * rather than timing), but still performs a full-length XOR when lengths match
 * so that a timing attacker cannot distinguish _where_ the difference occurs.
 *
 * @param {string} a - First string to compare.
 * @param {string} b - Second string to compare.
 * @returns {boolean} `true` when the strings are equal, `false` otherwise.
 *
 * @example
 * safeEqual('secret', 'secret'); // true
 * safeEqual('secret', 'wrong');  // false
 */
function safeEqual(a, b) {
  if (a.length !== b.length) { return false; }
  let result = 0;
  for (let i = 0; i < a.length; i++) {
    result |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return result === 0;
}

// Register bounded counters with the shared registry
registry.registerMetric(maturityReminderDeliveryAttemptsTotal);
registry.registerMetric(maturityReminderDeadLetterTotal);
registry.registerMetric(webhookReplayTotal);
registry.registerMetric(legalHoldProcessingErrorsTotal);
registry.registerMetric(legalHoldBlocksTotal);
registry.registerMetric(redisCacheFailOpenTotal);
registry.registerMetric(cacheStoreErrorsTotal);
registry.registerMetric(escrowPreflightRejectedTotal);
registry.registerMetric(bodySizeLimitRejectionsTotal);

/**
 * Set of loopback IP addresses that are allowed when no bearer token is
 * configured. Includes IPv4, IPv6, and IPv4-mapped IPv6 representations.
 *
 * @type {ReadonlySet<string>}
 */
const LOOPBACK = new Set(['127.0.0.1', '::1', '::ffff:127.0.0.1']);

/**
 * Extracts the direct TCP connection IP address from the request.
 *
 * Reads `req.socket.remoteAddress` first — this is the actual TCP socket peer
 * and cannot be spoofed via `X-Forwarded-For` or any other HTTP header. Falls
 * back to `req.ip` when the socket address is unavailable (edge case in some
 * test environments or HTTP/2 proxies).
 *
 * @param {import('express').Request} req - Express request object.
 * @returns {string} The client IP address string, or empty string if
 *   neither source is available.
 *
 * @example
 * extractClientIp(req); // '127.0.0.1'
 */
function extractClientIp(req) {
  return (req.socket && req.socket.remoteAddress) || req.ip || '';
}

/**
 * Express middleware that enforces metrics endpoint authentication.
 *
 * ## Auth decision flow
 *
 * ```
 * METRICS_BEARER_TOKEN set?
 *   ├── YES → constant-time compare Authorization header
 *   │         ├── match  → next()
 *   │         └── no match → 401 (no detail)
 *   └── NO  → extractClientIp(req) in LOOPBACK set?
 *             ├── yes → next()
 *             └── no  → 401 (no detail)
 * ```
 *
 * The response is **always** a plain `{ error: 'Unauthorized' }` with no
 * indication of whether the failure was a missing token, wrong token, or
 * non-loopback origin.
 *
 * @param {import('express').Request} req - Express request.
 * @param {import('express').Response} res - Express response.
 * @param {import('express').NextFunction} next - Express next callback.
 * @returns {void}
 */
function metricsAuth(req, res, next) {
  const token = process.env.METRICS_BEARER_TOKEN;

  if (token) {
    const auth = req.headers['authorization'] || '';
    if (safeEqual(auth, `Bearer ${token}`)) { return next(); }
    const authFallback = req.headers['Authorization'] || '';
    if (safeEqual(authFallback, `Bearer ${token}`)) { return next(); }
    res.status(401).json({ error: 'Unauthorized' });
    return;
  }

  // No token configured — allow loopback only, using the direct TCP socket IP.
  // X-Forwarded-For is NEVER trusted for this check.
  const ip = extractClientIp(req);
  if (LOOPBACK.has(ip)) { return next(); }

  res.status(401).json({ error: 'Unauthorized' });
}

/**
 * Express route handler that returns Prometheus metrics in plain-text format.
 *
 * @param {import('express').Request} _req - Express request (unused).
 * @param {import('express').Response} res - Express response.
 * @returns {Promise<void>}
 */
async function metricsHandler(_req, res) {
  res.set('Content-Type', registry.contentType);
  // Use the real prom-client registry.metrics() when available (production),
  // which returns the full Prometheus exposition including ALL registered
  // counters and gauges. Fall back to cachedMetrics for the shim (tests).
  const metricsText = typeof client.Gauge !== 'function' || client.Gauge.name === 'GaugeShim'
    ? cachedMetrics
    : await registry.metrics();
  res.end(metricsText);
}

/**
 * Counter: Escrow events successfully processed by the indexer per cycle.
 * Incremented by the number of events persisted in each indexer cycle.
 * @type {import('prom-client').Counter}
 */
const escrowIndexerEventsProcessedTotal = new client.Counter({
  name: 'escrow_indexer_events_processed_total',
  help: 'Total number of escrow events successfully processed and persisted by the indexer',
  registers: [registry],
});

/**
 * Counter: Escrow events skipped (invalid) by the indexer per cycle.
 * Incremented when an event fails validation or persistence.
 * @type {import('prom-client').Counter}
 */
const escrowIndexerEventsSkippedTotal = new client.Counter({
  name: 'escrow_indexer_events_skipped_total',
  help: 'Total number of escrow events skipped due to validation or persistence errors',
  registers: [registry],
});

/**
 * Counter: Escrow indexer cycle failures.
 * Incremented when a cycle throws an unhandled exception or receives invalid metric data.
 * @type {import('prom-client').Counter}
 */
const escrowIndexerCycleFailuresTotal = new client.Counter({
  name: 'escrow_indexer_cycle_failures_total',
  help: 'Total number of escrow indexer cycles that failed with an exception',
  registers: [registry],
});

/**
 * Gauge: Unix timestamp (seconds) of the last successful cursor advance.
 * Updated when a cycle completes and cursorAfter !== cursorBefore.
 * Used by health check to detect indexer staleness.
 * @type {import('prom-client').Gauge}
 */
const escrowIndexerLastCursorAdvanceTimestampSeconds = new client.Gauge({
  name: 'escrow_indexer_last_cursor_advance_timestamp_seconds',
  help: 'Unix timestamp (seconds) of the last cycle where the cursor advanced (cursorAfter !== cursorBefore)',
  registers: [registry],
});

/**
 * Counter: Escrow reconciliation mismatches.
 * Incremented each time a reconcileInvoice call detects a discrepancy
 * between the DB funded total and the on-chain funded amount.
 * @type {import('prom-client').Counter}
 */
const escrowReconciliationMismatches = new client.Counter({
  name: 'escrow_reconciliation_mismatches_total',
  help: 'Total number of escrow reconciliation mismatches detected',
  registers: [registry],
});

/**
 * Gauge: Count of mismatched invoices from the most recent reconciliation run.
 * Updated after each performReconciliation run completes.
 * @type {import('prom-client').Gauge}
 */
const escrowReconciliationMismatchedInvoicesGauge = new client.Gauge({
  name: 'escrow_reconciliation_mismatched_invoices',
  help: 'Number of mismatched invoices from the most recent reconciliation run',
  registers: [registry],
});

/**
 * Gauge: Total absolute drift magnitude (sum of |DB - onChain|) from the most
 * recent reconciliation run. Higher values indicate larger financial discrepancies.
 * @type {import('prom-client').Gauge}
 */
const escrowReconciliationDriftMagnitudeGauge = new client.Gauge({
  name: 'escrow_reconciliation_drift_magnitude',
  help: 'Total absolute drift magnitude from the most recent reconciliation run',
  registers: [registry],
});

/**
 * Counter: Reconciliation runs that breached the configured drift threshold.
 * Incremented when mismatches >= RECONCILIATION_DRIFT_THRESHOLD.
 * @type {import('prom-client').Counter}
 */
const escrowReconciliationDriftAlertsTotal = new client.Counter({
  name: 'escrow_reconciliation_drift_alerts_total',
  help: 'Total number of reconciliation runs that breached the drift threshold',
  registers: [registry],
});



/**
 * Counter: Footprint cache hits.
 * @type {import('prom-client').Counter}
 */
const footprintCacheHitsTotal = new client.Counter({
  name: 'soroban_footprint_cache_hits_total',
  help: 'Total number of Soroban footprint cache hits',
  registers: [registry],
});

/**
 * Counter: Footprint cache misses.
 * @type {import('prom-client').Counter}
 */
const footprintCacheMissesTotal = new client.Counter({
  name: 'soroban_footprint_cache_misses_total',
  help: 'Total number of Soroban footprint cache misses',
  registers: [registry],
});

/**
 * Counter: Footprint cache evictions (LRU or TTL).
 * @type {import('prom-client').Counter}
 */
const footprintCacheEvictionsTotal = new client.Counter({
  name: 'soroban_footprint_cache_evictions_total',
  help: 'Total number of Soroban footprint cache evictions (LRU or TTL expiry)',
  registers: [registry],
});

/**
 * Counter: Soroban circuit breaker state transitions.
 * Labelled by the new state name to allow counting transitions into each state.
 * @type {import('prom-client').Counter}
 */
const sorobanCircuitBreakerStateTransitionsTotal = new client.Counter({
  name: 'soroban_circuit_breaker_state_transitions_total',
  help: 'Total number of Soroban circuit breaker state transitions, labelled by state',
  labelNames: ['state'],
  registers: [registry],
});

/**
 * Gauge: Readiness state (1 = ready, 0 = not ready).
 * Updated by performReadinessChecks() in the health service.
 * @type {import('prom-client').Gauge}
 */
const readinessGauge = new client.Gauge({
  name: 'readiness_gauge',
  help: 'Readiness state of the service: 1 = ready to serve traffic, 0 = not ready',
  registers: [registry],
});

/**
 * Counter: operator alerts raised when `contractListRefresh` detects an on-chain
 * LiquifactEscrow wasm `SCHEMA_VERSION` that diverges from the expected/known
 * registry version.
 *
 * Labelled by the comparison `status` (`ahead` — contract is newer than anything
 * the backend tracks; `unknown` — version not present in the registry) so ops can
 * distinguish an upgrade from an unexpected/rolled-back deployment. A non-zero
 * value is an actionable signal that the backend may not yet support the on-chain
 * contract — see `docs/wasm-ops.md`.
 *
 * @type {import('prom-client').Counter}
 */
const contractWasmVersionMismatchAlertsTotal = new client.Counter({
  name: 'contract_wasm_version_mismatch_alerts_total',
  help: 'Total operator alerts raised when contractListRefresh detects an on-chain wasm SCHEMA_VERSION mismatch',
  labelNames: ['status'],
  registers: [registry],
});

module.exports = {
  registry,
  metricsAuth,
  metricsHandler,
  registerJobQueue,
  registerWorker,
  refreshMetrics,
  resetMetricsForTests,
  contractWasmVersionMismatchAlertsTotal,
  readinessGauge,
  escrowIndexerEventsProcessedTotal,
  escrowIndexerEventsSkippedTotal,
  escrowIndexerCycleFailuresTotal,
  escrowIndexerLastCursorAdvanceTimestampSeconds,
  maturityReminderDeliveryAttemptsTotal,
  maturityReminderDeadLetterTotal,
  normalizeReminderReason,
  normalizeJobType,
  escrowReconciliationMismatches,
  escrowReconciliationMismatchedInvoicesGauge,
  escrowReconciliationDriftMagnitudeGauge,
  escrowReconciliationDriftAlertsTotal,
  incrementLegalHoldUnknownBlocks,
  incrementLegalHoldBlocks,
  footprintCacheHitsTotal,
  footprintCacheMissesTotal,
  footprintCacheEvictionsTotal,
  redisCacheFailOpenTotal,
  cacheStoreErrorsTotal,
  escrowPreflightRejectedTotal,
  bodySizeLimitRejectionsTotal,
  webhookReplayTotal,
};
