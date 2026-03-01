import { useEffect, useRef, useState, useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';

/**
 * Custom hook that polls a callback at a specified interval.
 * Automatically cleans up on unmount and handles callback changes.
 */
export function usePolling(callback: () => void, intervalMs: number, enabled: boolean = true) {
  const callbackRef = useRef(callback);

  // Always keep the ref up to date so interval calls the latest callback
  useEffect(() => {
    callbackRef.current = callback;
  });

  useEffect(() => {
    if (!enabled) return;

    const id = setInterval(() => {
      callbackRef.current();
    }, intervalMs);

    return () => clearInterval(id);
  }, [intervalMs, enabled]);
}

// --- useLogStream ---

interface UseLogStreamOptions {
  url: string;
  enabled?: boolean;
}

interface UseLogStreamResult {
  lines: string[];
  isConnected: boolean;
  isPaused: boolean;
  error: string | null;
  pause: () => void;
  resume: () => void;
  clear: () => void;
  setLines: Dispatch<SetStateAction<string[]>>;
}

/**
 * Custom hook that manages an EventSource connection for streaming log lines.
 * Handles connection lifecycle, pause/resume, error recovery, and cleanup.
 */
export function useLogStream({ url, enabled = true }: UseLogStreamOptions): UseLogStreamResult {
  const [lines, setLines] = useState<string[]>([]);
  const [isActive, setIsActive] = useState(true);
  const [isPaused, setIsPaused] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset streaming state when URL changes
  useEffect(() => {
    setIsActive(true);
    setError(null);
  }, [url]);

  useEffect(() => {
    if (!enabled || !isActive || isPaused) return;

    const eventSource = new EventSource(url);

    eventSource.onmessage = (event: MessageEvent) => {
      setLines((prev) => [...prev.slice(-2000), event.data as string]);
    };

    eventSource.onerror = () => {
      console.error("Log stream error");
      setError("Log stream connection failed");
      setIsActive(false);
      eventSource.close();
    };

    return () => {
      eventSource.close();
    };
  }, [url, enabled, isActive, isPaused]);

  const pause = useCallback(() => setIsPaused(true), []);
  const resume = useCallback(() => setIsPaused(false), []);
  const clear = useCallback(() => setLines([]), []);

  return {
    lines,
    isConnected: isActive && !isPaused && enabled,
    isPaused,
    error,
    pause,
    resume,
    clear,
    setLines,
  };
}
