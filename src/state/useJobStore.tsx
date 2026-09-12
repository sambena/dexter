import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";

export interface Job {
  label: string;
  /** Null when the job can't report how far along it is. */
  current: number | null;
  total: number | null;
  detail: string | null;
  cancellable: boolean;
}

interface JobContextValue {
  job: Job | null;
  startJob: (label: string, options?: { cancellable?: boolean }) => void;
  updateJob: (update: { current?: number; total?: number; detail?: string }) => void;
  endJob: () => void;
}

const JobContext = createContext<JobContextValue | null>(null);

export function JobProvider({ children }: { children: ReactNode }) {
  const [job, setJob] = useState<Job | null>(null);
  // Nested/overlapping jobs shouldn't let an inner one clear the outer bar.
  const depth = useRef(0);

  const startJob = useCallback((label: string, options?: { cancellable?: boolean }) => {
    depth.current += 1;
    setJob({ label, current: null, total: null, detail: null, cancellable: options?.cancellable ?? false });
  }, []);

  const updateJob = useCallback((update: { current?: number; total?: number; detail?: string }) => {
    setJob((prev) =>
      prev
        ? {
            ...prev,
            current: update.current ?? prev.current,
            total: update.total ?? prev.total,
            detail: update.detail ?? prev.detail,
          }
        : prev,
    );
  }, []);

  const endJob = useCallback(() => {
    depth.current = Math.max(0, depth.current - 1);
    if (depth.current === 0) setJob(null);
  }, []);

  const value = useMemo(() => ({ job, startJob, updateJob, endJob }), [job, startJob, updateJob, endJob]);
  return <JobContext.Provider value={value}>{children}</JobContext.Provider>;
}

export function useJob(): JobContextValue {
  const ctx = useContext(JobContext);
  if (!ctx) throw new Error("useJob must be used inside a JobProvider");
  return ctx;
}
