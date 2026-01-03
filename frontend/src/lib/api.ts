/**
 * API client for ore-stats server
 * 
 * Handles:
 * - Rate limiting (respects server limits)
 * - Authentication (admin routes)
 * - Error handling
 */

import { API_URL, API_RATE_LIMIT } from "./constants";

// ============================================================================
// Types
// ============================================================================

export interface ApiError {
  error: string;
}

// Admin types
export interface AdminMetrics {
  uptime_seconds: number;
  current_slot: number;
  miners_cached: number;
  ore_holders_cached: number;
  pending_round_id: number;
  pending_deployments: number;
}

export interface LoginResponse {
  token: string;
  expires_at: string;
}

export interface BlacklistEntry {
  ip_address: string;
  reason: string;
  failed_attempts: number;
  blocked_at: string;
  expires_at: string | null;
  created_by: string | null;
}

export interface BlacklistResponse {
  entries: BlacklistEntry[];
  total: number;
}

export interface RpcSummaryRow {
  program: string;
  provider: string;
  method: string;
  target_type: string;
  total_requests: number;
  success_count: number;
  error_count: number;
  timeout_count: number;
  rate_limited_count: number;
  not_found_count: number;
  total_operations: number;
  total_results: number;
  avg_duration_ms: number;
  max_duration_ms: number;
  min_duration_ms: number;
  total_request_bytes: number;
  total_response_bytes: number;
}

export interface RpcProviderRow {
  program: string;
  provider: string;
  total_requests: number;
  success_count: number;
  error_count: number;
  timeout_count: number;
  rate_limited_count: number;
  total_operations: number;
  total_results: number;
  avg_duration_ms: number;
  max_duration_ms: number;
  total_request_bytes: number;
  total_response_bytes: number;
}

export interface RpcErrorRow {
  timestamp: number; // milliseconds since epoch (DateTime64(3))
  program: string;
  provider: string;
  method: string;
  target_type: string;
  target_address: string;
  status: string;
  error_code: string;
  error_message: string;
  duration_ms: number;
}

export interface RpcTimeseriesRow {
  minute: number; // seconds since epoch (DateTime)
  total_requests: number;
  success_count: number;
  error_count: number;
  timeout_count: number;
  total_operations: number;
  total_results: number;
  avg_duration_ms: number;
  max_duration_ms: number;
}

export interface RpcDailyRow {
  day: number; // days since 1970-01-01 (Date)
  program: string;
  provider: string;
  total_requests: number;
  success_count: number;
  error_count: number;
  rate_limited_count: number;
  total_operations: number;
  total_results: number;
  avg_duration_ms: number;
  total_request_bytes: number;
  total_response_bytes: number;
  unique_methods: number;
}

// Individual RPC request (all requests, not just errors)
export interface RpcRequestRow {
  timestamp: number; // milliseconds since epoch
  program: string;
  provider: string;
  method: string;
  target_type: string;
  target_address: string;
  is_batch: number;
  batch_size: number;
  status: string;
  error_code: string;
  error_message: string;
  result_count: number;
  filters_json: string; // JSON string of filters (memcmp, dataSize)
  duration_ms: number;
  request_size: number;
  response_size: number;
}

// WebSocket metrics types
export interface WsEventRow {
  timestamp: number; // milliseconds since epoch
  program: string;
  provider: string;
  subscription_type: string;
  subscription_key: string;
  event: string;
  error_message: string;
  disconnect_reason: string;
  uptime_seconds: number;
  messages_received: number;
  reconnect_count: number;
}

export interface WsThroughputSummary {
  program: string;
  provider: string;
  subscription_type: string;
  total_messages: number;
  total_bytes: number;
  avg_process_time_us: number;
}

// Backfill workflow types
export interface BackfillStartResponse {
  message: string;
  stop_at_round: number;
  max_pages: number;
}

export type BackfillTaskStatus = "idle" | "running" | "completed" | "failed" | "cancelled";

export interface BackfillRoundsTaskState {
  status: BackfillTaskStatus;
  started_at_ms: number | null;
  stop_at_round: number;
  max_pages: number;
  current_page: number;
  per_page: number;
  rounds_fetched: number;
  rounds_skipped: number;
  rounds_not_in_external_api: number;
  pages_jumped: number;
  last_round_id_processed: number | null;
  first_round_id_seen: number | null;
  estimated_total_rounds: number | null;
  error: string | null;
  elapsed_ms: number;
  estimated_remaining_ms: number | null;
  last_updated: string;
}

export interface QueuedAction {
  id: number;
  round_id: number;
  action: string;
  status: string;
  queued_at: string;
  started_at: string | null;
  completed_at: string | null;
  error: string | null;
}

export interface QueueStatus {
  paused: boolean;
  pending_count: number;
  processing: QueuedAction | null;
  total_processed: number;
  total_failed: number;
  processing_rate: number;
  recent_completed: QueuedAction[];
  recent_failed: QueuedAction[];
}

export interface PipelineStats {
  not_in_workflow: number;
  pending_txns: number;
  pending_reconstruct: number;
  pending_verify: number;
  pending_finalize: number;
  complete: number;
}

export interface MemoryUsage {
  memory_bytes: number;
  memory_human: string;
  queue_cache_items: number;
}

export interface BulkEnqueueRequest {
  start_round: number;
  end_round: number;
  action: string;
  skip_if_done: boolean;
  only_in_workflow: boolean;
}

export interface BulkEnqueueResponse {
  message: string;
  enqueued: number;
  pages_jumped: number;
}

// Legacy response type (kept for backward compat)
export interface BackfillRoundsResponse {
  rounds_fetched: number;
  rounds_skipped: number;
  rounds_missing_deployments: number;
  stopped_at_round: number | null;
}

export interface RoundWithData {
  round_id: number;
  start_slot: number;
  end_slot: number;
  winning_square: number;
  top_miner: string;
  total_deployed: number;
  total_winnings: number;
  unique_miners: number;
  motherlode: number;
  deployment_count: number;
  source: string;
  deployments_sum?: number;
  is_valid?: boolean;
  discrepancy?: number;
}

export interface RoundsWithDataResponse {
  rounds: RoundWithData[];
  total: number;
  has_more: boolean;
  next_cursor?: number;
  page?: number;
}

export interface MissingRoundsResponse {
  missing_round_ids: number[];
  total: number;
  has_more: boolean;
  next_cursor?: number;
  page?: number;
  min_stored_round: number;
  max_stored_round: number;
}

export interface RoundStatsResponse {
  total_rounds: number;
  missing_deployments_count: number;
  invalid_deployments_count: number;
  missing_rounds_count: number;
  min_stored_round: number;
  max_stored_round: number;
}

export type FilterMode = "all" | "missing_deployments" | "invalid_deployments";

export interface RoundStatus {
  round_id: number;
  meta_fetched: boolean;
  transactions_fetched: boolean;
  reconstructed: boolean;
  verified: boolean;
  finalized: boolean;
  transaction_count: number;
  deployment_count: number;
  verification_notes: string;
}

export interface PendingRoundsResponse {
  pending: RoundStatus[];
  total: number;
}

export interface FetchTxnsResponse {
  round_id: number;
  transactions_fetched: number;
  status: string;
}

export interface DeployEventCoverageResponse {
  round_id: number;
  /** True if all Deploy instructions have corresponding DeployEvents */
  has_all_events: boolean;
  /** Number of Deploy instructions found */
  deploy_count: number;
  /** Number of DeployEvents found for this round */
  event_count: number;
  /** Number of deploys missing events */
  deploys_without_events: number;
  /** True if automation states need to be fetched */
  needs_automation_states: boolean;
  /** Details of deploys missing events */
  missing_deploys: DeployWithoutEventInfo[];
}

export interface DeployWithoutEventInfo {
  signature: string;
  authority: string;
  slot: number;
}

export interface ReconstructResponse {
  round_id: number;
  deployments_reconstructed: number;
  status: string;
}

export interface VerifyResponse {
  round_id: number;
  verified: boolean;
  message: string;
}

export interface FinalizeResponse {
  round_id: number;
  deployments_stored: number;
  message: string;
}

export interface DeleteResponse {
  round_id: number;
  round_deleted: boolean;
  deployments_deleted: boolean;
  message: string;
}

export interface BulkDeleteResponse {
  deleted_count: number;
  failed_count: number;
  message: string;
}

// Add to backfill workflow types
export interface AddToBackfillResponse {
  added: number;
  already_pending: number;
  message: string;
}

export interface RoundDataStatus {
  round_id: number;
  round_exists: boolean;
  deployment_count: number;
}

// Server metrics types
export interface ServerMetricsRow {
  timestamp: number; // seconds since epoch
  requests_total: number;
  requests_success: number;
  requests_error: number;
  latency_p50: number;
  latency_p95: number;
  latency_p99: number;
  latency_avg: number;
  active_connections: number;
  memory_used: number;
  cache_hits: number;
  cache_misses: number;
}

export interface RequestsPerMinuteRow {
  minute_ts: number; // Unix timestamp (start of minute)
  request_count: number;
  success_count: number;
  error_count: number;
  avg_latency_ms: number;
}

export interface RequestsTimeseriesResponse {
  hours: number;
  rps: number;
  timeseries: RequestsPerMinuteRow[];
}

// Request logs types
export interface RequestLogRow {
  timestamp: number; // milliseconds since epoch
  endpoint: string;
  method: string;
  status_code: number;
  duration_ms: number;
  ip_hash: string;
  user_agent: string;
}

export interface EndpointSummaryRow {
  endpoint: string;
  total_requests: number;
  success_count: number;
  error_count: number;
  avg_duration_ms: number;
  max_duration_ms: number;
  p95_duration_ms: number;
}

export interface RateLimitEventRow {
  timestamp: number; // milliseconds since epoch
  ip_hash: string;
  endpoint: string;
  requests_in_window: number;
  window_seconds: number;
}

export interface IpActivityRow {
  ip_hash: string;
  total_requests: number;
  error_count: number;
  rate_limit_count: number;
  avg_duration_ms: number;
}

// Database size types
export interface StorageSummary {
  total_bytes: number;
  total_rows: number;
  clickhouse_bytes: number;
  postgres_bytes: number;
  compression_ratio: number;
}

export interface DatabaseSizeRow {
  database: string;
  bytes_on_disk: number;
  total_rows: number;
  table_count: number;
}

export interface DetailedTable {
  database: string;
  table: string;
  bytes_on_disk: number;
  bytes_uncompressed: number;
  compression_ratio: number;
  total_rows: number;
  parts_count: number;
  last_modified: string;
  avg_row_size: number;
}

export interface TableEngineRow {
  database: string;
  table: string;
  engine: string;
  partition_key: string;
  sorting_key: string;
  primary_key: string;
}

export interface PostgresTableSize {
  table_name: string;
  total_size_bytes: number;
  table_size_bytes: number;
  index_size_bytes: number;
  row_count: number;
  avg_row_size: number;
  dead_tuples: number;
  last_vacuum: string | null;
  last_analyze: string | null;
}

export interface ClickHouseSizes {
  databases: DatabaseSizeRow[];
  tables: DetailedTable[];
  engines: TableEngineRow[];
  total_bytes: number;
  total_bytes_uncompressed: number;
  total_rows: number;
}

export interface PostgresSizes {
  database_name: string;
  database_size_bytes: number;
  table_sizes: PostgresTableSize[];
  total_rows: number;
}

export interface DatabaseSizesResponse {
  summary: StorageSummary;
  clickhouse: ClickHouseSizes;
  postgres: PostgresSizes;
}

export interface TransactionMigrationStats {
  raw_transactions_count: number;
  raw_transactions_rounds: number;
  signatures_count: number;
  raw_transactions_v2_count: number;
  unmigrated_count: number;
  next_round_to_migrate: number | null;
  migration_progress_pct: number;
}

// ORE data types (cached by ore-stats)
export interface Board {
  round_id: number;
  start_slot: number;
  end_slot: number;
}

export interface Treasury {
  balance: number;
  rewards_factor: string;
  last_reset_at: number;
}

export interface Round {
  round_id: number;
  start_slot: number;
  end_slot: number;
  slots_remaining: number;
  deployed: number[];
  count: number[];
  total_deployed: number;
  unique_miners: number;
}

export interface Miner {
  pubkey: string;
  authority: string;
  round_id: number;
  deployed: number[];
  rewards_sol: number;
  rewards_ore: number;
}

// ============================================================================
// Historical Data Types (Phase 3)
// ============================================================================

export interface HistoricalRound {
  round_id: number;
  start_slot: number;
  end_slot: number;
  winning_square: number;
  top_miner: string;
  total_deployed: number;
  total_vaulted: number;
  total_winnings: number;
  unique_miners: number;
  motherlode: number;
  motherlode_hit: boolean;
  created_at: string;
}

export interface HistoricalDeployment {
  round_id: number;
  miner_pubkey: string;
  square_id: number;
  amount: number;
  deployed_slot: number;
  sol_earned: number;
  ore_earned: number;
  is_winner: boolean;
  is_top_miner: boolean;
  winning_square: number;
}

export interface MinerStats {
  miner_pubkey: string;
  total_deployed: number;
  total_sol_earned: number;
  total_ore_earned: number;
  net_sol_change: number;
  rounds_played: number;
  rounds_won: number;
  win_rate: number;
  avg_deployment: number;
  avg_slots_left: number;
}

export interface MinerSquareStats {
  miner_pubkey: string;
  /** Number of deployments to each square (25 elements, indexed by square_id) */
  square_counts: number[];
  /** Total amount deployed to each square in lamports (25 elements) */
  square_amounts: number[];
  /** Number of wins on each square (25 elements) */
  square_wins: number[];
  /** Total unique rounds the miner participated in */
  total_rounds: number;
}

export interface LeaderboardEntry {
  rank: number;
  miner_pubkey: string;
  value: number;
  rounds_played: number;
  sol_deployed: number;
  sol_earned: number;
  ore_earned: number;
  net_sol: number;
  sol_cost_per_ore: number | null;
}

export interface CostPerOreStats {
  /** Total number of rounds in the range */
  total_rounds: number;
  /** Total SOL vaulted across all rounds (lamports) */
  total_vaulted_lamports: number;
  /** Total ORE minted (atomic units, 11 decimals) = rounds + motherlode ORE */
  total_ore_minted_atomic: number;
  /** Cost per ORE in lamports (total_vaulted / total_ore) */
  cost_per_ore_lamports: number;
}

export interface TreasurySnapshot {
  round_id: number;
  balance: number;
  motherlode: number;
  total_staked: number;
  total_unclaimed: number;
  total_refined: number;
  created_at: string;
}

export interface MinerSnapshotEntry {
  miner_pubkey: string;
  refined_ore: number;
  unclaimed_ore: number;
  lifetime_sol: number;
  lifetime_ore: number;
}

export interface MinerSnapshotsResponse {
  round_id: number;
  data: MinerSnapshotEntry[];
  page: number;
  per_page: number;
  total_count: number;
  total_pages: number;
}

export interface CursorResponse<T> {
  data: T[];
  cursor: string | null;
  has_more: boolean;
}

export interface OffsetResponse<T> {
  data: T[];
  page: number;
  per_page: number;
  total_count: number;
  total_pages: number;
}

// ============================================================================
// Rate Limiter
// ============================================================================

class RateLimiter {
  private lastRequestTime = 0;
  private minDelayMs: number;

  constructor(requestsPerSecond: number) {
    this.minDelayMs = 1000 / requestsPerSecond;
  }

  async wait(): Promise<void> {
    const now = Date.now();
    const timeSinceLastRequest = now - this.lastRequestTime;
    
    if (timeSinceLastRequest < this.minDelayMs) {
      const waitTime = this.minDelayMs - timeSinceLastRequest;
      await new Promise(resolve => setTimeout(resolve, waitTime));
    }
    
    this.lastRequestTime = Date.now();
  }
}

const rateLimiter = new RateLimiter(API_RATE_LIMIT.requestsPerSecond);

// ============================================================================
// API Client
// ============================================================================

class ApiClient {
  private baseUrl: string;
  private authToken: string | null = null;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/$/, ""); // Remove trailing slash
    
    // Try to restore token from localStorage
    if (typeof window !== "undefined") {
      this.authToken = localStorage.getItem("admin_token");
    }
  }

  setAuthToken(token: string | null) {
    this.authToken = token;
    if (typeof window !== "undefined") {
      if (token) {
        localStorage.setItem("admin_token", token);
      } else {
        localStorage.removeItem("admin_token");
      }
    }
  }

  getAuthToken(): string | null {
    return this.authToken;
  }

  isAuthenticated(): boolean {
    return this.authToken !== null;
  }

  private async request<T>(
    method: string,
    path: string,
    options?: {
      body?: unknown;
      requireAuth?: boolean;
      skipRateLimit?: boolean;
    }
  ): Promise<T> {
    // Rate limit unless explicitly skipped
    if (!options?.skipRateLimit) {
      await rateLimiter.wait();
    }

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };

    if (options?.requireAuth && this.authToken) {
      headers["Authorization"] = `Bearer ${this.authToken}`;
    }

    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: options?.body ? JSON.stringify(options.body) : undefined,
    });

    // Handle 401 - clear auth and throw
    if (response.status === 401) {
      this.setAuthToken(null);
      throw new Error("Unauthorized - please login again");
    }

    if (!response.ok) {
      const error: ApiError = await response.json().catch(() => ({ 
        error: `HTTP ${response.status}: ${response.statusText}` 
      }));
      throw new Error(error.error || `Request failed: ${response.status}`);
    }

    return response.json();
  }

  // ========== Public Endpoints ==========

  async getHealth(): Promise<{ status: string }> {
    return this.request("GET", "/health");
  }

  async getBoard(): Promise<Board> {
    return this.request("GET", "/board");
  }

  async getTreasury(): Promise<Treasury> {
    return this.request("GET", "/treasury");
  }

  async getRound(): Promise<Round> {
    return this.request("GET", "/round");
  }

  async getSlot(): Promise<{ slot: number }> {
    return this.request("GET", "/slot");
  }

  async getMiners(): Promise<Miner[]> {
    return this.request("GET", "/miners");
  }

  async getMiner(pubkey: string): Promise<Miner> {
    return this.request("GET", `/miner/${pubkey}`);
  }

  async getBalance(pubkey: string): Promise<{ balance: number }> {
    return this.request("GET", `/balance/${pubkey}`);
  }

  async getOreBalance(owner: string): Promise<{ balance: number }> {
    return this.request("GET", `/ore-balance/${owner}`);
  }

  async getMetrics(): Promise<Record<string, unknown>> {
    return this.request("GET", "/metrics");
  }

  // ========== Admin Endpoints ==========

  async login(password: string): Promise<LoginResponse> {
    const response = await this.request<LoginResponse>("POST", "/admin/login", {
      body: { password },
    });
    this.setAuthToken(response.token);
    return response;
  }

  async logout(): Promise<void> {
    try {
      await this.request("POST", "/admin/logout", { requireAuth: true });
    } finally {
      this.setAuthToken(null);
    }
  }

  async getAdminMetrics(): Promise<AdminMetrics> {
    return this.request("GET", "/admin/metrics", { requireAuth: true });
  }

  async getRpcSummary(hours = 24): Promise<{ hours: number; data: RpcSummaryRow[] }> {
    return this.request("GET", `/admin/rpc?hours=${hours}`, { requireAuth: true });
  }

  async getRpcProviders(hours = 24): Promise<{ hours: number; providers: RpcProviderRow[] }> {
    return this.request("GET", `/admin/rpc/providers?hours=${hours}`, { requireAuth: true });
  }

  async getRpcErrors(hours = 24, limit = 100): Promise<{ hours: number; limit: number; errors: RpcErrorRow[] }> {
    return this.request("GET", `/admin/rpc/errors?hours=${hours}&limit=${limit}`, { requireAuth: true });
  }

  async getRpcTimeseries(hours = 24): Promise<{ hours: number; timeseries: RpcTimeseriesRow[] }> {
    return this.request("GET", `/admin/rpc/timeseries?hours=${hours}`, { requireAuth: true });
  }

  async getRpcDaily(days = 7): Promise<{ days: number; daily: RpcDailyRow[] }> {
    return this.request("GET", `/admin/rpc/daily?days=${days}`, { requireAuth: true });
  }

  async getRpcRequests(hours = 24, limit = 100): Promise<{ hours: number; limit: number; requests: RpcRequestRow[] }> {
    return this.request("GET", `/admin/rpc/requests?hours=${hours}&limit=${limit}`, { requireAuth: true });
  }

  async getBlacklist(): Promise<BlacklistResponse> {
    return this.request("GET", "/admin/blacklist", { requireAuth: true });
  }

  async addToBlacklist(ip: string, reason: string, permanent = false): Promise<{ message: string }> {
    return this.request("POST", "/admin/blacklist", {
      body: { ip, reason, permanent },
      requireAuth: true,
    });
  }

  async removeFromBlacklist(ip: string): Promise<{ message: string }> {
    return this.request("DELETE", `/admin/blacklist/${encodeURIComponent(ip)}`, { requireAuth: true });
  }

  async cleanupSessions(): Promise<{ message: string }> {
    return this.request("POST", "/admin/sessions/cleanup", { requireAuth: true });
  }

  // ========== WebSocket Metrics ==========

  async getWsEvents(hours = 24, limit = 100): Promise<{ hours: number; events: WsEventRow[] }> {
    return this.request("GET", `/admin/ws/events?hours=${hours}&limit=${limit}`, { requireAuth: true });
  }

  async getWsThroughput(hours = 24): Promise<{ hours: number; throughput: WsThroughputSummary[] }> {
    return this.request("GET", `/admin/ws/throughput?hours=${hours}`, { requireAuth: true });
  }

  // ========== Server Metrics ==========

  async getServerMetrics(hours = 24, limit = 100): Promise<{ hours: number; metrics: ServerMetricsRow[] }> {
    return this.request("GET", `/admin/server/metrics?hours=${hours}&limit=${limit}`, { requireAuth: true });
  }

  async getRequestsTimeseries(hours = 24): Promise<RequestsTimeseriesResponse> {
    return this.request("GET", `/admin/server/requests-timeseries?hours=${hours}`, { requireAuth: true });
  }

  // ========== Request Logs ==========

  async getRequestLogs(options?: {
    hours?: number;
    limit?: number;
    ipHash?: string;
    endpoint?: string;
    statusCode?: number;
    statusGte?: number;
    statusLte?: number;
  }): Promise<{ hours: number; logs: RequestLogRow[] }> {
    const params = new URLSearchParams();
    params.set("hours", (options?.hours ?? 24).toString());
    params.set("limit", (options?.limit ?? 500).toString());
    if (options?.ipHash) params.set("ip_hash", options.ipHash);
    if (options?.endpoint) params.set("endpoint", options.endpoint);
    if (options?.statusCode) params.set("status_code", options.statusCode.toString());
    if (options?.statusGte) params.set("status_gte", options.statusGte.toString());
    if (options?.statusLte) params.set("status_lte", options.statusLte.toString());
    return this.request("GET", `/admin/requests/logs?${params.toString()}`, { requireAuth: true });
  }

  async getEndpointSummary(hours = 24): Promise<{ hours: number; endpoints: EndpointSummaryRow[] }> {
    return this.request("GET", `/admin/requests/endpoints?hours=${hours}`, { requireAuth: true });
  }

  async getRateLimitEvents(hours = 24, limit = 100): Promise<{ hours: number; events: RateLimitEventRow[] }> {
    return this.request("GET", `/admin/requests/rate-limits?hours=${hours}&limit=${limit}`, { requireAuth: true });
  }

  async getIpActivity(hours = 24, limit = 50): Promise<{ hours: number; activity: IpActivityRow[] }> {
    return this.request("GET", `/admin/requests/ip-activity?hours=${hours}&limit=${limit}`, { requireAuth: true });
  }

  async getDatabaseSizes(): Promise<DatabaseSizesResponse> {
    return this.request("GET", "/admin/database/sizes", { requireAuth: true });
  }

  async getTransactionMigrationStats(): Promise<TransactionMigrationStats> {
    return this.request("GET", "/admin/database/transaction-migration", { requireAuth: true });
  }

  // ========== Backfill Workflow ==========

  async backfillRounds(stopAtRound?: number, maxPages?: number): Promise<BackfillStartResponse> {
    const params = new URLSearchParams();
    if (stopAtRound) params.set("stop_at_round", stopAtRound.toString());
    if (maxPages) params.set("max_pages", maxPages.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("POST", `/admin/backfill/rounds${query}`, { requireAuth: true });
  }

  async getBackfillRoundsStatus(): Promise<BackfillRoundsTaskState> {
    return this.request("GET", "/admin/backfill/rounds/status", { requireAuth: true });
  }

  async cancelBackfillRounds(): Promise<{ message: string }> {
    return this.request("POST", "/admin/backfill/rounds/cancel", { requireAuth: true });
  }

  // Queue management
  async getQueueStatus(): Promise<QueueStatus> {
    return this.request("GET", "/admin/backfill/queue/status", { requireAuth: true });
  }

  async getPipelineStats(): Promise<PipelineStats> {
    return this.request("GET", "/admin/backfill/pipeline-stats", { requireAuth: true });
  }

  async getMemoryUsage(): Promise<MemoryUsage> {
    return this.request("GET", "/admin/backfill/memory", { requireAuth: true });
  }

  async pauseQueue(): Promise<{ message: string }> {
    return this.request("POST", "/admin/backfill/queue/pause", { requireAuth: true });
  }

  async resumeQueue(): Promise<{ message: string }> {
    return this.request("POST", "/admin/backfill/queue/resume", { requireAuth: true });
  }

  async clearQueue(): Promise<{ message: string; cleared: number }> {
    return this.request("POST", "/admin/backfill/queue/clear", { requireAuth: true });
  }

  async retryFailedQueue(): Promise<{ message: string; retried: number }> {
    return this.request("POST", "/admin/backfill/queue/retry-failed", { requireAuth: true });
  }

  async enqueueActions(request: BulkEnqueueRequest): Promise<BulkEnqueueResponse> {
    return this.request("POST", "/admin/backfill/queue/enqueue", { requireAuth: true, body: request });
  }

  async addRangeToWorkflow(startRound: number, endRound: number): Promise<{ message: string; added: number }> {
    return this.request("POST", "/admin/backfill/add-range", { requireAuth: true, body: { start_round: startRound, end_round: endRound } });
  }

  async bulkVerifyRounds(roundIds: number[]): Promise<{ message: string; verified: number }> {
    return this.request("POST", "/admin/backfill/bulk-verify", { requireAuth: true, body: { round_ids: roundIds } });
  }

  async getPendingRounds(): Promise<PendingRoundsResponse> {
    return this.request("GET", "/admin/rounds/pending", { requireAuth: true });
  }

  async fetchRoundTransactions(roundId: number): Promise<FetchTxnsResponse> {
    return this.request("POST", `/admin/fetch-txns/${roundId}`, { requireAuth: true });
  }

  async resetTxnsStatus(roundId: number): Promise<{ round_id: number; message: string }> {
    return this.request("POST", `/admin/reset-txns/${roundId}`, { requireAuth: true });
  }

  async reconstructRound(roundId: number): Promise<ReconstructResponse> {
    return this.request("POST", `/admin/reconstruct/${roundId}`, { requireAuth: true });
  }

  async getRoundForVerification(roundId: number): Promise<RoundStatus> {
    return this.request("GET", `/admin/verify/${roundId}`, { requireAuth: true });
  }

  async verifyRound(roundId: number, notes?: string): Promise<VerifyResponse> {
    return this.request("POST", `/admin/verify/${roundId}`, { requireAuth: true, body: { notes } });
  }

  async finalizeRound(roundId: number): Promise<FinalizeResponse> {
    return this.request("POST", `/admin/finalize/${roundId}`, { requireAuth: true });
  }

  async getRoundDataStatus(roundId: number): Promise<RoundDataStatus> {
    return this.request("GET", `/admin/rounds/${roundId}/status`, { requireAuth: true });
  }

  /**
   * Check if all Deploy instructions in a round have corresponding DeployEvents.
   * If all events exist, automation state fetching can be skipped for reconstruction.
   */
  async checkDeployEventCoverage(roundId: number): Promise<DeployEventCoverageResponse> {
    return this.request("GET", `/admin/deploy-events/${roundId}`, { requireAuth: true });
  }

  async deleteRoundData(roundId: number, deleteRound = false, deleteDeployments = true): Promise<DeleteResponse> {
    const params = new URLSearchParams();
    params.set("delete_round", deleteRound.toString());
    params.set("delete_deployments", deleteDeployments.toString());
    return this.request("DELETE", `/admin/rounds/${roundId}?${params.toString()}`, { requireAuth: true });
  }

  async getRoundsWithData(options?: {
    limit?: number;
    page?: number;
    before?: number;
    roundIdGte?: number;
    roundIdLte?: number;
    filterMode?: FilterMode;
  }): Promise<RoundsWithDataResponse> {
    const params = new URLSearchParams();
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.page) params.set("page", options.page.toString());
    if (options?.before) params.set("before", options.before.toString());
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    if (options?.filterMode) params.set("filter_mode", options.filterMode);
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/rounds/data${query}`, { requireAuth: true });
  }
  
  async getMissingRounds(options?: {
    limit?: number;
    page?: number;
    roundIdGte?: number;
    roundIdLte?: number;
  }): Promise<MissingRoundsResponse> {
    const params = new URLSearchParams();
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.page) params.set("page", options.page.toString());
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/rounds/missing${query}`, { requireAuth: true });
  }
  
  async getRoundStats(options?: {
    roundIdGte?: number;
    roundIdLte?: number;
  }): Promise<RoundStatsResponse> {
    const params = new URLSearchParams();
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/rounds/stats${query}`, { requireAuth: true });
  }

  async bulkDeleteRounds(roundIds: number[], deleteRounds: boolean, deleteDeployments: boolean): Promise<BulkDeleteResponse> {
    return this.request("POST", "/admin/rounds/bulk-delete", {
      requireAuth: true,
      body: {
        round_ids: roundIds,
        delete_rounds: deleteRounds,
        delete_deployments: deleteDeployments,
      },
    });
  }

  async addToBackfillWorkflow(roundIds: number[]): Promise<AddToBackfillResponse> {
    return this.request("POST", "/admin/backfill/deployments", {
      requireAuth: true,
      body: {
        round_ids: roundIds,
      },
    });
  }

  // ========== Historical Data Endpoints (Phase 3) ==========

  async getHistoricalRounds(options?: {
    cursor?: string;
    limit?: number;
    roundIdGte?: number;
    roundIdLte?: number;
    motherlodeHit?: boolean;
    order?: "asc" | "desc";
  }): Promise<CursorResponse<HistoricalRound>> {
    const params = new URLSearchParams();
    if (options?.cursor) params.set("cursor", options.cursor);
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    if (options?.motherlodeHit !== undefined) params.set("motherlode_hit", options.motherlodeHit.toString());
    if (options?.order) params.set("order", options.order);
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/rounds${query}`);
  }

  async getHistoricalRound(roundId: number): Promise<HistoricalRound> {
    return this.request("GET", `/history/rounds/${roundId}`);
  }

  async getRoundDeployments(roundId: number, options?: {
    cursor?: string;
    limit?: number;
    miner?: string;
    winnerOnly?: boolean;
  }): Promise<CursorResponse<HistoricalDeployment>> {
    const params = new URLSearchParams();
    if (options?.cursor) params.set("cursor", options.cursor);
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.miner) params.set("miner", options.miner);
    if (options?.winnerOnly) params.set("winner_only", "true");
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/rounds/${roundId}/deployments${query}`);
  }

  async getDeployments(options?: {
    cursor?: string;
    limit?: number;
    roundIdGte?: number;
    roundIdLte?: number;
    miner?: string;
    winnerOnly?: boolean;
  }): Promise<CursorResponse<HistoricalDeployment>> {
    const params = new URLSearchParams();
    if (options?.cursor) params.set("cursor", options.cursor);
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    if (options?.miner) params.set("miner", options.miner);
    if (options?.winnerOnly) params.set("winner_only", "true");
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/deployments${query}`);
  }

  async getMinerDeployments(pubkey: string, options?: {
    cursor?: string;
    limit?: number;
    roundIdGte?: number;
    roundIdLte?: number;
    winnerOnly?: boolean;
    baseOreOnly?: boolean;
    motherlodeOnly?: boolean;
  }): Promise<CursorResponse<HistoricalDeployment>> {
    const params = new URLSearchParams();
    if (options?.cursor) params.set("cursor", options.cursor);
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    if (options?.winnerOnly) params.set("winner_only", "true");
    if (options?.baseOreOnly) params.set("base_ore_only", "true");
    if (options?.motherlodeOnly) params.set("motherlode_only", "true");
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/miner/${pubkey}/deployments${query}`);
  }

  async getMinerStats(pubkey: string, options?: {
    roundIdGte?: number;
    roundIdLte?: number;
  }): Promise<MinerStats> {
    const params = new URLSearchParams();
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/miner/${pubkey}/stats${query}`);
  }

  async getMinerSquareStats(pubkey: string, options?: {
    roundIdGte?: number;
    roundIdLte?: number;
  }): Promise<MinerSquareStats> {
    const params = new URLSearchParams();
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/miner/${pubkey}/square-stats${query}`);
  }

  async getLeaderboard(options?: {
    metric?: "net_sol" | "sol_deployed" | "sol_earned" | "ore_earned" | "sol_cost";
    roundIdGte?: number;
    roundIdLte?: number;
    page?: number;
    limit?: number;
    search?: string;
    minRounds?: number;
  }): Promise<OffsetResponse<LeaderboardEntry>> {
    const params = new URLSearchParams();
    if (options?.metric) params.set("metric", options.metric);
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    if (options?.page) params.set("page", options.page.toString());
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.search) params.set("search", options.search);
    if (options?.minRounds) params.set("min_rounds", options.minRounds.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/leaderboard${query}`);
  }

  async getCostPerOreStats(options?: {
    roundIdGte?: number;
    roundIdLte?: number;
  }): Promise<CostPerOreStats> {
    const params = new URLSearchParams();
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/rounds/cost-per-ore${query}`);
  }

  async getTreasuryHistory(options?: {
    cursor?: string;
    limit?: number;
    roundIdGte?: number;
    roundIdLte?: number;
  }): Promise<CursorResponse<TreasurySnapshot>> {
    const params = new URLSearchParams();
    if (options?.cursor) params.set("cursor", options.cursor);
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.roundIdGte) params.set("round_id_gte", options.roundIdGte.toString());
    if (options?.roundIdLte) params.set("round_id_lte", options.roundIdLte.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/treasury/history${query}`);
  }

  async getMinerSnapshots(options?: {
    roundId?: number;
    sortBy?: "refined_ore" | "unclaimed_ore" | "lifetime_sol" | "lifetime_ore";
    order?: "desc" | "asc";
    page?: number;
    limit?: number;
    search?: string;
  }): Promise<MinerSnapshotsResponse> {
    const params = new URLSearchParams();
    if (options?.roundId) params.set("round_id", options.roundId.toString());
    if (options?.sortBy) params.set("sort_by", options.sortBy);
    if (options?.order) params.set("order", options.order);
    if (options?.page) params.set("page", options.page.toString());
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.search) params.set("search", options.search);
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/history/miners${query}`);
  }

  // Transaction Viewer
  async getTransactionAnalysis(roundId: number, options?: {
    limit?: number;
    offset?: number;
  }): Promise<TransactionViewerResponse> {
    const params = new URLSearchParams();
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.offset) params.set("offset", options.offset.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/transactions/${roundId}${query}`, { requireAuth: true });
  }

  async getRawTransactions(roundId: number): Promise<RawTransaction[]> {
    return this.request("GET", `/admin/transactions/${roundId}/raw`, { requireAuth: true });
  }

  // Comprehensive Transaction Analyzer
  async getFullTransactionAnalysis(roundId: number, options?: {
    limit?: number;
    offset?: number;
  }): Promise<FullAnalysisResponse> {
    const params = new URLSearchParams();
    if (options?.limit) params.set("limit", options.limit.toString());
    if (options?.offset) params.set("offset", options.offset.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/transactions/${roundId}/full${query}`, { requireAuth: true });
  }

  async getSingleTransaction(signature: string): Promise<FullTransactionAnalysis> {
    return this.request("GET", `/admin/transactions/single/${signature}`, { requireAuth: true });
  }

  async getRoundsWithTransactions(page = 1, limit = 50): Promise<RoundsWithTransactionsResponse> {
    return this.request("GET", `/admin/transactions/rounds?page=${page}&limit=${limit}`, { requireAuth: true });
  }

  // ========== Automation State Reconstruction ==========

  async getAutomationQueueStats(): Promise<AutomationQueueStats> {
    return this.request("GET", "/admin/automation/stats", { requireAuth: true });
  }

  async getAutomationFetchStats(): Promise<AutomationFetchStats> {
    return this.request("GET", "/admin/automation/fetch-stats", { requireAuth: true });
  }

  async getAutomationLiveStats(): Promise<AutomationLiveStats | null> {
    return this.request<AutomationLiveStats>("GET", "/admin/automation/live", { requireAuth: true }).catch(() => null);
  }

  async getAutomationQueue(options?: {
    status?: string;
    round_id?: number;
    authority?: string;
    page?: number;
    limit?: number;
  }): Promise<AutomationQueueResponse> {
    const params = new URLSearchParams();
    if (options?.status) params.set("status", options.status);
    if (options?.round_id) params.set("round_id", options.round_id.toString());
    if (options?.authority) params.set("authority", options.authority);
    if (options?.page) params.set("page", options.page.toString());
    if (options?.limit) params.set("limit", options.limit.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/automation/queue${query}`, { requireAuth: true });
  }

  async processAutomationQueue(count = 5): Promise<AutomationProcessResult> {
    return this.request("POST", `/admin/automation/queue/process?count=${count}`, { requireAuth: true });
  }

  async retryFailedAutomation(): Promise<{ retried: number }> {
    return this.request("POST", "/admin/automation/queue/retry", { requireAuth: true });
  }

  async queueAutomationForRound(roundId: number): Promise<AutomationAddToQueueResponse> {
    return this.request("POST", `/admin/automation/queue/round/${roundId}`, { requireAuth: true });
  }

  async queueAutomationFromTransactions(roundId: number): Promise<AutomationAddToQueueResponse> {
    return this.request("POST", `/admin/automation/queue/from-txns/${roundId}`, { requireAuth: true });
  }
  
  // New queue-based system
  async queueRoundForParsing(roundId: number): Promise<QueueRoundResponse> {
    return this.request("POST", `/admin/automation/queue-round/${roundId}`, { requireAuth: true });
  }
  
  async getParseQueueStats(): Promise<ParseQueueStats> {
    return this.request("GET", "/admin/automation/parse-queue", { requireAuth: true });
  }
  
  async getParseQueueItems(options?: { status?: string; limit?: number }): Promise<ParseQueueItem[]> {
    const params = new URLSearchParams();
    if (options?.status) params.set("status", options.status);
    if (options?.limit) params.set("limit", options.limit.toString());
    const query = params.toString() ? `?${params.toString()}` : "";
    return this.request("GET", `/admin/automation/parse-queue/items${query}`, { requireAuth: true });
  }

  // ========== Public Chart Endpoints ==========

  async getChartRoundsHourly(hours: number = 24): Promise<RoundsHourlyData[]> {
    return this.request("GET", `/charts/rounds/hourly?hours=${hours}`);
  }

  async getChartRoundsDaily(days: number = 30): Promise<RoundsDailyData[]> {
    return this.request("GET", `/charts/rounds/daily?days=${days}`);
  }

  async getChartTreasuryHourly(hours: number = 24): Promise<TreasuryHourlyData[]> {
    return this.request("GET", `/charts/treasury/hourly?hours=${hours}`);
  }

  async getChartMintHourly(hours: number = 24): Promise<MintHourlyData[]> {
    return this.request("GET", `/charts/mint/hourly?hours=${hours}`);
  }

  async getChartMintDaily(days: number = 30): Promise<MintDailyData[]> {
    return this.request("GET", `/charts/mint/daily?days=${days}`);
  }

  async getChartInflationHourly(hours: number = 24): Promise<InflationHourlyData[]> {
    return this.request("GET", `/charts/inflation/hourly?hours=${hours}`);
  }

  async getChartInflationDaily(days: number = 30): Promise<InflationDailyData[]> {
    return this.request("GET", `/charts/inflation/daily?days=${days}`);
  }

  async getChartCostPerOreDaily(days: number = 30): Promise<CostPerOreDailyData[]> {
    return this.request("GET", `/charts/cost-per-ore/daily?days=${days}`);
  }

  async getChartMinersDaily(days: number = 30): Promise<MinerActivityDailyData[]> {
    return this.request("GET", `/charts/miners/daily?days=${days}`);
  }

  // ========== Direct/Round-based Chart Endpoints ==========

  async getChartRoundsDirect(
    start?: number,
    end?: number | "live",
    limit: number = 1000
  ): Promise<DirectResponse<RoundDirectData>> {
    const params = new URLSearchParams();
    if (start !== undefined) params.set("start", String(start));
    if (end !== undefined) params.set("end", String(end));
    params.set("limit", String(limit));
    return this.request("GET", `/charts/rounds/direct?${params.toString()}`);
  }

  async getChartTreasuryDirect(
    start?: number,
    end?: number | "live",
    limit: number = 1000
  ): Promise<DirectResponse<TreasuryDirectData>> {
    const params = new URLSearchParams();
    if (start !== undefined) params.set("start", String(start));
    if (end !== undefined) params.set("end", String(end));
    params.set("limit", String(limit));
    return this.request("GET", `/charts/treasury/direct?${params.toString()}`);
  }

  async getChartMintDirect(
    start?: number,
    end?: number | "live",
    limit: number = 1000
  ): Promise<DirectResponse<MintDirectData>> {
    const params = new URLSearchParams();
    if (start !== undefined) params.set("start", String(start));
    if (end !== undefined) params.set("end", String(end));
    params.set("limit", String(limit));
    return this.request("GET", `/charts/mint/direct?${params.toString()}`);
  }

  async getChartInflationDirect(
    start?: number,
    end?: number | "live",
    limit: number = 1000
  ): Promise<DirectResponse<InflationDirectData>> {
    const params = new URLSearchParams();
    if (start !== undefined) params.set("start", String(start));
    if (end !== undefined) params.set("end", String(end));
    params.set("limit", String(limit));
    return this.request("GET", `/charts/inflation/direct?${params.toString()}`);
  }

  async getChartCostPerOreDirect(
    start?: number,
    end?: number | "live",
    limit: number = 1000
  ): Promise<DirectResponse<CostPerOreDirectData>> {
    const params = new URLSearchParams();
    if (start !== undefined) params.set("start", String(start));
    if (end !== undefined) params.set("end", String(end));
    params.set("limit", String(limit));
    return this.request("GET", `/charts/cost-per-ore/direct?${params.toString()}`);
  }
}

// Transaction Viewer Types
export interface TransactionViewerResponse {
  round_id: number;
  total_transactions: number;
  transactions: TransactionAnalysis[];
  summary: TransactionSummary;
}

export interface TransactionSummary {
  total_txns: number;
  with_deploy_ix: number;
  without_deploy_ix: number;
  parse_errors: number;
  wrong_round: number;
  matched_round: number;
  total_deployments: number;
}

export interface TransactionAnalysis {
  signature: string;
  slot: number;
  block_time: number;
  signer: string | null;
  has_ore_program: boolean;
  instructions_count: number;
  inner_instructions_count: number;
  deploy_instructions: DeployInstructionAnalysis[];
  other_ore_instructions: OtherOreInstruction[];
  parse_errors: string[];
  status: string;
}

export interface DeployInstructionAnalysis {
  location: string;
  instruction_index: number;
  signer: string;
  authority: string;
  miner: string;
  round_pda: string;
  amount_per_square: number;
  squares_mask: number;
  squares: number[];
  matches_expected_round: boolean;
}

export interface OtherOreInstruction {
  location: string;
  instruction_index: number;
  instruction_tag: number;
  instruction_name: string;
}

export interface RawTransaction {
  signature: string;
  slot: number;
  block_time: number;
  round_id: number;
  tx_type: string;
  raw_json: string;
  signer: string;
  authority: string;
}

// ============================================================================
// Comprehensive Transaction Analyzer Types
// ============================================================================

export interface FullAnalysisResponse {
  round_id: number;
  total_transactions: number;
  analyzed_count: number;
  transactions: FullTransactionAnalysis[];
  round_summary: RoundAnalysisSummary;
  missing_automation_states: MissingAutomationState[];
  failed_transactions: FailedTransaction[];
}

export interface MissingAutomationState {
  signature: string;
  ix_index: number;
  miner: string;
  authority: string;
}

export interface FailedTransaction {
  signature: string;
  slot: number;
  error: string;
}

export interface FullTransactionAnalysis {
  signature: string;
  slot: number;
  block_time: number;
  block_time_formatted: string;
  success: boolean;
  error: string | null;
  fee: number;
  compute_units_consumed: number | null;
  signers: string[];
  writable_accounts: string[];
  readonly_accounts: string[];
  all_accounts: AccountInfo[];
  balance_changes: BalanceChange[];
  programs_invoked: ProgramInfo[];
  instructions: InstructionAnalysis[];
  inner_instructions: InnerInstructionGroup[];
  logs: string[];
  ore_analysis: OreTransactionAnalysis | null;
  summary: TransactionSummaryInfo;
}

export interface AccountInfo {
  index: number;
  pubkey: string;
  is_signer: boolean;
  is_writable: boolean;
  is_program: boolean;
  program_name: string | null;
  pre_balance: number;
  post_balance: number;
  balance_change: number;
}

export interface BalanceChange {
  account: string;
  pre_balance: number;
  post_balance: number;
  change: number;
  change_sol: number;
}

export interface ProgramInfo {
  pubkey: string;
  name: string;
  invocation_count: number;
}

export interface InstructionAnalysis {
  index: number;
  program_id: string;
  program_name: string;
  instruction_type: string;
  accounts: InstructionAccount[];
  data_base58: string;
  data_hex: string;
  data_length: number;
  parsed: ParsedInstruction | null;
  parse_error: string | null;
}

export interface InstructionAccount {
  index: number;
  pubkey: string;
  is_signer: boolean;
  is_writable: boolean;
  role: string | null;
}

export interface InnerInstructionGroup {
  parent_index: number;
  instructions: InstructionAnalysis[];
}

// Parsed instruction types (tagged union)
export type ParsedInstruction =
  | { type: "SystemTransfer"; from: string; to: string; lamports: number; sol: number }
  | { type: "SystemCreateAccount"; from: string; new_account: string; lamports: number; space: number; owner: string }
  | { type: "SystemAssign"; account: string; owner: string }
  | { type: "SystemAllocate"; account: string; space: number }
  | { type: "SystemAdvanceNonceAccount"; nonce_account: string; nonce_authority: string }
  | { type: "ComputeSetLimit"; units: number }
  | { type: "ComputeSetPrice"; micro_lamports: number }
  | { type: "ComputeRequestHeapFrame"; bytes: number }
  | { type: "OreDeploy"; signer: string; authority: string; automation_pda: string; board: string; miner: string; round: string; round_id: number | null; amount_per_square: number; amount_sol: number; squares_mask: number; squares: number[]; total_lamports: number; total_sol: number }
  | { type: "OreCheckpoint"; signer: string; authority: string; miner: string; round: string; round_id: number | null }
  | { type: "OreClaim"; signer: string; authority: string; miner: string; beneficiary: string }
  | { type: "OreAutomate"; signer: string; authority: string; automation_pda: string }
  | { type: "OreReset"; signer: string; board: string }
  | { type: "OreLog"; event_type: string; data_hex: string }
  | { type: "OreOther"; instruction_tag: number; instruction_name: string; accounts_count: number }
  | { type: "TokenTransfer"; source: string; destination: string; authority: string; amount: number }
  | { type: "TokenTransferChecked"; source: string; destination: string; mint: string; authority: string; amount: number; decimals: number }
  | { type: "TokenInitializeAccount"; account: string; mint: string; owner: string }
  | { type: "TokenApprove"; source: string; delegate: string; owner: string; amount: number }
  | { type: "TokenMintTo"; mint: string; destination: string; authority: string; amount: number }
  | { type: "TokenBurn"; account: string; mint: string; authority: string; amount: number }
  | { type: "TokenCloseAccount"; account: string; destination: string; authority: string }
  | { type: "AtaCreate"; payer: string; associated_token: string; wallet: string; mint: string }
  | { type: "Memo"; message: string }
  | { type: "Unknown"; program: string; data_preview: string };

export interface OreTransactionAnalysis {
  has_ore_instructions: boolean;
  deploy_count: number;
  reset_count: number;
  log_count: number;
  other_count: number;
  deployments: OreDeploymentInfo[];
  total_deployed_lamports: number;
  total_deployed_sol: number;
  
  // Logged totals from text logs (parsed from "Round #X: deploying Y SOL to Z squares")
  logged_deployments: LoggedDeployment[];
  logged_deploy_count: number;
  logged_deployed_lamports: number;
  logged_deployed_sol: number;
}

export interface LoggedDeployment {
  round_id: number;
  amount_per_square_sol: number;
  squares_count: number;
  total_sol: number;
  total_lamports: number;
  round_matches: boolean;
  authority: string | null;
  matched_parsed: boolean;
}

export interface OreDeploymentInfo {
  instruction_index: number;
  is_inner: boolean;
  signer: string;
  authority: string;
  miner: string;
  round: string;
  round_id: number | null;
  expected_round_id: number | null;
  round_matches: boolean;
  amount_per_square: number;
  squares: number[];
  total_lamports: number;
  total_sol: number;
}

export interface TransactionSummaryInfo {
  total_instructions: number;
  total_inner_instructions: number;
  programs_count: number;
  has_failed: boolean;
  is_ore_transaction: boolean;
  is_deploy_transaction: boolean;
  primary_action: string;
}

export interface RoundAnalysisSummary {
  total_transactions: number;
  successful_transactions: number;
  failed_transactions: number;
  total_fee_paid: number;
  total_fee_sol: number;
  total_compute_units: number;
  unique_signers: number;
  programs_used: ProgramUsageSummary[];
  ore_summary: OreRoundSummary | null;
}

export interface ProgramUsageSummary {
  program: string;
  name: string;
  invocation_count: number;
}

export interface OreRoundSummary {
  total_deployments: number;
  deployments_matching_round: number;
  deployments_wrong_round: number;
  unique_miners: number;
  total_deployed_lamports: number;
  total_deployed_sol: number;
  squares_deployed: SquareDeploymentInfo[];
  
  // Logged totals from text logs ("Round #X: deploying Y SOL to Z squares")
  logged_deploy_count: number;
  logged_deployed_lamports: number;
  logged_deployed_sol: number;
  logged_unique_miners: number;
  // Logged deployments that couldn't be matched to a parsed instruction (indicates parsing issue)
  logged_unmatched_count: number;
  
  // Comparison: logged - parsed (positive = logged has more than parsed)
  logged_vs_parsed_diff_lamports: number;
  logged_vs_parsed_diff_sol: number;
}

export interface SquareDeploymentInfo {
  square: number;
  deployment_count: number;
  total_lamports: number;
}

export interface RoundTransactionInfo {
  round_id: number;
  transaction_count: number;
  min_slot: number;
  max_slot: number;
}

export interface RoundsWithTransactionsResponse {
  rounds: RoundTransactionInfo[];
  total: number;
  page: number;
  limit: number;
}

// ============================================================================
// Chart Data Types
// ============================================================================

export interface RoundsHourlyData {
  hour: number;  // Unix timestamp
  rounds_count: number;
  total_deployments: number;
  unique_miners: number;
  total_deployed: number;
  total_vaulted: number;
  total_winnings: number;
  motherlode_hits: number;
  total_motherlode: number;
}

export interface RoundsDailyData {
  day: number;  // Unix timestamp (midnight UTC)
  rounds_count: number;
  total_deployments: number;
  unique_miners: number;
  total_deployed: number;
  total_vaulted: number;
  total_winnings: number;
  motherlode_hits: number;
  total_motherlode: number;
}

export interface TreasuryHourlyData {
  hour: number;  // Unix timestamp
  balance: number;
  motherlode: number;
  total_staked: number;
  total_unclaimed: number;
  total_refined: number;
}

export interface MintHourlyData {
  hour: number;  // Unix timestamp
  supply_start: number;
  supply: number;
  supply_change_total: number;
  round_count: number;
}

export interface MintDailyData {
  day: number;  // Unix timestamp (midnight UTC)
  supply: number;
  supply_start: number;
  supply_change_total: number;
  round_count: number;
}

export interface InflationHourlyData {
  hour: number;  // Unix timestamp
  supply_end: number;
  supply_change_total: number;
  unclaimed_end: number;
  unclaimed_change_total: number;
  circulating_end: number;
  ore_won_total: number;
  ore_claimed_total: number;
  ore_burned_total: number;
  market_inflation_total: number;
  rounds_count: number;
}

export interface InflationDailyData {
  day: number;  // Unix timestamp (midnight UTC)
  supply_start: number;
  supply_end: number;
  supply_change_total: number;
  unclaimed_start: number;
  unclaimed_end: number;
  unclaimed_change_total: number;
  circulating_start: number;
  circulating_end: number;
  ore_won_total: number;
  ore_claimed_total: number;
  ore_burned_total: number;
  market_inflation_total: number;
  rounds_count: number;
}

export interface CostPerOreDailyData {
  day: number;  // Unix timestamp (midnight UTC)
  rounds_count: number;
  total_vaulted: number;
  ore_minted_total: number;
  cost_per_ore_lamports: number;
}

export interface MinerActivityDailyData {
  day: number;  // Unix timestamp (midnight UTC)
  active_miners: number;
  total_deployments: number;
  total_deployed: number;
  total_won: number;
}

// ============================================================================
// Direct/Round-based Chart Data Types
// ============================================================================

export interface DirectMeta {
  latest_round_id: number;
}

export interface DirectResponse<T> {
  meta: DirectMeta;
  data: T[];
}

export interface RoundDirectData {
  round_id: number;
  created_at: number;  // Unix timestamp ms
  total_deployments: number;
  unique_miners: number;
  total_deployed: number;
  total_vaulted: number;
  total_winnings: number;
  motherlode_hit: boolean;
  motherlode: number;
}

export interface TreasuryDirectData {
  round_id: number;
  created_at: number;  // Unix timestamp ms
  balance: number;
  motherlode: number;
  total_staked: number;
  total_unclaimed: number;
  total_refined: number;
}

export interface MintDirectData {
  round_id: number;
  created_at: number;  // Unix timestamp ms
  supply: number;
  supply_change: number;
}

export interface InflationDirectData {
  round_id: number;
  created_at: number;  // Unix timestamp ms
  supply: number;
  supply_change: number;
  unclaimed: number;
  circulating: number;
  market_inflation: number;
}

export interface CostPerOreDirectData {
  round_id: number;
  created_at: number;  // Unix timestamp ms
  total_vaulted: number;
  ore_minted: number;
  cost_per_ore_lamports: number;
}

// ============================================================================
// Automation State Reconstruction Types
// ============================================================================

export interface AutomationQueueStats {
  pending: number;
  processing: number;
  completed: number;
  failed: number;
  total: number;
  avg_fetch_duration_ms: number | null;
  avg_txns_searched: number | null;
}

export interface AutomationFetchStats {
  total_fetched: number;
  found_count: number;
  active_count: number;
  avg_txns_searched: number;
  avg_duration_ms: number;
  max_txns_searched: number;
  max_duration_ms: number;
  partial_deploy_count: number;
  total_sol_tracked: number;
}

export interface AutomationLiveStats {
  is_running: boolean;
  current_item_id: number | null;
  current_signature: string | null;
  current_authority: string | null;
  txns_searched_so_far: number;
  pages_fetched_so_far: number;
  elapsed_ms: number;
  items_processed_this_session: number;
  items_succeeded_this_session: number;
  items_failed_this_session: number;
  last_updated: string;
}

// Queue-based transaction parsing
export interface QueueRoundResponse {
  success: boolean;
  round_id: number;
  status: string;
  message: string;
}

export interface ParseQueueStats {
  pending: number;
  processing: number;
  completed: number;
  failed: number;
}

export interface ParseQueueItem {
  id: number;
  round_id: number;
  status: string;
  txns_found: number | null;
  deploys_queued: number | null;
  errors_count: number | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  last_error: string | null;
}

export interface AutomationQueueItem {
  id: number;
  round_id: number;
  miner_pubkey: string;
  authority_pubkey: string;
  automation_pda: string;
  deploy_signature: string;
  deploy_ix_index: number;
  deploy_slot: number;
  status: string;
  attempts: number;
  last_error: string | null;
  txns_searched: number | null;
  pages_fetched: number | null;
  fetch_duration_ms: number | null;
  automation_found: boolean | null;
  priority: number;
  created_at: string;
  updated_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface AutomationQueueResponse {
  items: AutomationQueueItem[];
  total: number;
  page: number;
  limit: number;
}

export interface AutomationProcessResult {
  processed: number;
  success: number;
  failed: number;
  details: AutomationProcessDetail[];
}

export interface AutomationProcessDetail {
  id: number;
  deploy_signature: string;
  success: boolean;
  automation_found: boolean;
  automation_active: boolean;
  txns_searched: number;
  duration_ms: number;
  used_cache: boolean;
  cache_slot: number | null;
  error: string | null;
}

export interface AutomationAddToQueueResponse {
  queued: number;
  already_exists: number;
  errors: string[];
}

// ========== External API (kriptikz.dev) ==========

export interface ExternalDeployment {
  round_id: number;
  pubkey: string;           // miner authority
  deployments: number[];    // 25 values, lamports per square
  sol_deployed: number;     // total lamports
  sol_earned: number;
  ore_earned: number;
}

export interface ExternalComparisonSummary {
  unique_miners: number;
  total_lamports: number;
  total_sol: number;
  deployments: ExternalDeployment[];
}

/**
 * Fetch deployment data from external API (kriptikz.dev)
 * Used for comparing with parsed/logged data to validate accuracy
 */
export async function fetchExternalDeployments(
  roundId: number,
  timeoutMs: number = 30000
): Promise<{ data: ExternalDeployment[] | null; error: string | null }> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
  
  try {
    const response = await fetch(
      `https://kriptikz.dev/v2/deployments?round_id=${roundId}`,
      { signal: controller.signal }
    );
    
    clearTimeout(timeoutId);
    
    if (!response.ok) {
      return { data: null, error: `HTTP ${response.status}: ${response.statusText}` };
    }
    
    const data: ExternalDeployment[] = await response.json();
    return { data, error: null };
  } catch (err) {
    clearTimeout(timeoutId);
    
    if (err instanceof Error) {
      if (err.name === 'AbortError') {
        return { data: null, error: 'Request timed out' };
      }
      return { data: null, error: err.message };
    }
    return { data: null, error: 'Unknown error occurred' };
  }
}

/**
 * Calculate comparison summary from external deployments
 */
export function calculateExternalSummary(deployments: ExternalDeployment[]): ExternalComparisonSummary {
  const uniqueMiners = new Set(deployments.map(d => d.pubkey));
  const totalLamports = deployments.reduce((sum, d) => sum + d.sol_deployed, 0);
  
  return {
    unique_miners: uniqueMiners.size,
    total_lamports: totalLamports,
    total_sol: totalLamports / 1e9,
    deployments,
  };
}

// Singleton instance
export const api = new ApiClient(API_URL);

