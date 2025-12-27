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
  total_requests: number;
  success_count: number;
  error_count: number;
  timeout_count: number;
  rate_limited_count: number;
  avg_duration_ms: number;
  max_duration_ms: number;
  total_request_bytes: number;
  total_response_bytes: number;
}

export interface RpcProviderRow {
  program: string;
  provider: string;
  total_requests: number;
  success_count: number;
  error_count: number;
  rate_limited_count: number;
  avg_duration_ms: number;
  total_request_bytes: number;
  total_response_bytes: number;
}

export interface RpcErrorRow {
  timestamp: string; // ISO 8601 datetime
  program: string;
  provider: string;
  method: string;
  status: string;
  error_code: string;
  error_message: string;
  duration_ms: number;
}

export interface RpcTimeseriesRow {
  minute: string; // ISO 8601 datetime
  total_requests: number;
  success_count: number;
  error_count: number;
  avg_duration_ms: number;
}

export interface RpcDailyRow {
  day: string; // ISO 8601 date
  program: string;
  provider: string;
  total_requests: number;
  success_count: number;
  error_count: number;
  rate_limited_count: number;
  avg_duration_ms: number;
  total_request_bytes: number;
  total_response_bytes: number;
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
}

// Singleton instance
export const api = new ApiClient(API_URL);

