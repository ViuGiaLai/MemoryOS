import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Matches the Rust IndexProgress struct. */
interface ScanProgress {
  total_files: number;
  indexed_files: number;
  failed_files: number;
  current_file: string | null;
  percentage: number;
  is_running: boolean;
}

/** Status snapshot derived from progress for the UI. */
export type ScanStatus = "idle" | "scanning" | "completed" | "error" | "stopped";

const defaultProgress: ScanProgress = {
  total_files: 0,
  indexed_files: 0,
  failed_files: 0,
  current_file: null,
  percentage: 0,
  is_running: false,
};

/** Hook to manage scanning lifecycle and polling. */
export function useScan() {
  const [progress, setProgress] = useState<ScanProgress>(defaultProgress);
  const [scanning, setScanning] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Derive status from progress using useMemo (no useEffect chain anti-pattern)
  const status: ScanStatus = useMemo(() => {
    if (!scanning && progress.is_running) return "idle"; // transient
    if (scanning && progress.is_running) return "scanning";
    if (!scanning) {
      if (progress.current_file?.startsWith("Lỗi") || progress.current_file?.startsWith("Error")) {
        return "error";
      }
      if (progress.percentage >= 100) return "completed";
      if (progress.total_files > 0 || progress.indexed_files > 0 || progress.failed_files > 0) {
        return "stopped";
      }
    }
    return "idle";
  }, [progress.is_running, progress.percentage, progress.current_file, progress.total_files, progress.indexed_files, progress.failed_files, scanning]);

  // Start polling progress when scanning
  useEffect(() => {
    if (scanning) {
      pollRef.current = setInterval(async () => {
        try {
          const p = await invoke<ScanProgress>("get_scan_progress");
          setProgress(p);
          // Stop polling when scan completes
          if (!p.is_running) {
            setScanning(false);
          }
        } catch (e) {
          console.error("Failed to poll scan progress:", e);
        }
      }, 500);
    }

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [scanning]);

  /** Start a new scan. */
  const startScan = useCallback(async () => {
    try {
      setScanning(true);
      setProgress({
        total_files: 0,
        indexed_files: 0,
        failed_files: 0,
        current_file: "Đang khởi tạo...",
        percentage: 0,
        is_running: true,
      });
      await invoke("start_scan");
    } catch (e) {
      console.error("Failed to start scan:", e);
      setScanning(false);
      setProgress((prev) => ({
        ...prev,
        is_running: false,
        current_file: `Lỗi: ${e}`,
      }));
    }
  }, []);

  /** Stop the running scan and fetch final progress. */
  const stopScan = useCallback(async () => {
    try {
      await invoke("stop_scan");
      // Fetch one more time to get the "Đã dừng" progress before clearing the interval
      const p = await invoke<ScanProgress>("get_scan_progress");
      setProgress(p);
    } catch (e) {
      console.error("Failed to stop scan:", e);
    } finally {
      setScanning(false);
    }
  }, []);

  /** Reset status back to idle. */
  const resetStatus = useCallback(() => {
    setScanning(false);
    setProgress(defaultProgress);
  }, []);

  return {
    progress,
    status,
    startScan,
    stopScan,
    resetStatus,
  };
}
