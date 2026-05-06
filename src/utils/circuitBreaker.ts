/**
 * circuitBreaker.ts
 *
 * A per-adapter circuit-breaker that tracks consecutive failures.
 * After THRESHOLD failures the circuit opens — callers skip the request
 * entirely and return a synthetic "offline" result instead of hammering
 * a dead remote host mid-service.
 *
 * The circuit closes again when the operator calls reset() (triggered by
 * the manual "Check" button on each adapter panel).
 *
 * Usage:
 *   const vmixBreaker = createCircuitBreaker("vmix");
 *   const result = await vmixBreaker.call(() => getVmixStatus());
 */

export type CircuitState = "closed" | "open";

export interface CircuitBreaker {
  /** Call fn if circuit is closed. If open, returns undefined immediately. */
  call<T>(fn: () => Promise<T>): Promise<T | undefined>;
  /** Manually reset (close) the circuit — call this from manual "Check" buttons. */
  reset(): void;
  /** Read-only state for UI indicators. */
  readonly state: CircuitState;
  readonly failureCount: number;
}

/** Number of consecutive failures before the circuit opens. */
const THRESHOLD = 3;

/** How long (ms) an open circuit auto-half-opens to allow a test probe. */
const AUTO_RECOVER_MS = 60_000;

const breakers = new Map<string, CircuitBreakerImpl>();

class CircuitBreakerImpl implements CircuitBreaker {
  private _state: CircuitState = "closed";
  private _failureCount = 0;
  private _openedAt: number | null = null;
  private readonly _name: string;
  private readonly _onStateChange?: (name: string, state: CircuitState) => void;

  constructor(name: string, onStateChange?: (name: string, state: CircuitState) => void) {
    this._name = name;
    this._onStateChange = onStateChange;
  }

  get state(): CircuitState { return this._state; }
  get failureCount(): number { return this._failureCount; }

  async call<T>(fn: () => Promise<T>): Promise<T | undefined> {
    // Auto half-open: allow a probe after AUTO_RECOVER_MS
    if (this._state === "open" && this._openedAt !== null) {
      if (Date.now() - this._openedAt >= AUTO_RECOVER_MS) {
        this._state = "closed"; // half-open probe
      } else {
        return undefined;
      }
    }

    try {
      const result = await fn();
      // Success — reset failure count
      this._failureCount = 0;
      if (this._state !== "closed") {
        this._state = "closed";
        this._openedAt = null;
        this._onStateChange?.(this._name, "closed");
      }
      return result;
    } catch (err) {
      this._failureCount++;
      if (this._failureCount >= THRESHOLD) {
        this._state = "open";
        this._openedAt = Date.now();
        this._onStateChange?.(this._name, "open");
        console.warn(
          `[CircuitBreaker] "${this._name}" opened after ${this._failureCount} failures.`,
          err
        );
      }
      throw err;
    }
  }

  reset(): void {
    this._failureCount = 0;
    this._state = "closed";
    this._openedAt = null;
    this._onStateChange?.(this._name, "closed");
  }
}

/**
 * Get (or create) a named CircuitBreaker instance.
 * Calling with the same name always returns the same breaker,
 * so state persists across component re-renders.
 */
export function createCircuitBreaker(
  name: string,
  onStateChange?: (name: string, state: CircuitState) => void
): CircuitBreaker {
  if (!breakers.has(name)) {
    breakers.set(name, new CircuitBreakerImpl(name, onStateChange));
  }
  return breakers.get(name)!;
}

/** Reset all breakers — call from global error recovery / app restart. */
export function resetAllBreakers(): void {
  breakers.forEach((b) => b.reset());
}
