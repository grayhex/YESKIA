import { useCallback, useRef, useState } from "react";
import { toErrorInfo, type Backend } from "../api";
import type { ErrorInfo, OperationRequest, Progress } from "../types";
import { baseName, splitName, withName } from "./paths";

export type TransferStatus = "pending" | "running" | "done" | "failed" | "skipped" | "cancelled";

export interface TransferItem {
  id: number;
  kind: "download" | "upload" | "mkdir";
  /** Отображаемое имя (меняется при «сохранить под другим именем»). */
  name: string;
  /** Источник: путь устройства (скачивание) или путь на компьютере (загрузка). */
  from: string;
  /** Назначение: путь на компьютере (скачивание/папка) или путь устройства (загрузка). */
  to: string;
  status: TransferStatus;
  bytes?: number;
  sha256?: string;
  replaced?: boolean;
  progress?: Progress | null;
  error?: ErrorInfo | null;
  /** Исход записи на устройство неизвестен. */
  uncertain?: boolean;
}

export type PlanItem = Pick<TransferItem, "kind" | "name" | "from" | "to">;
export interface ConflictChoice {
  action: "skip" | "rename" | "replace";
  /** Применить выбор ко всем следующим коллизиям этой очереди. */
  all: boolean;
}

/**
 * Очередь передачи файлов. Последовательна, **без автоповторов**: коллизия имени требует решения
 * пользователя (пропустить / другое имя / явная замена), неопределённый сбой записи или
 * отключение останавливают очередь.
 */
export function useTransfers(backend: Backend, deviceId: string | null, onFinished: () => void) {
  const [items, setItems] = useState<TransferItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [conflict, setConflict] = useState<{
    item: TransferItem;
    resolve: (c: ConflictChoice) => void;
  } | null>(null);
  const stop = useRef(false);
  const currentOp = useRef<string | null>(null);
  const seq = useRef(1);
  const busyRef = useRef(false);

  const update = useCallback((id: number, patch: Partial<TransferItem>) => {
    setItems((all) => all.map((i) => (i.id === id ? { ...i, ...patch } : i)));
  }, []);

  const askConflict = (item: TransferItem) =>
    new Promise<ConflictChoice>((resolve) => setConflict({ item, resolve }));

  async function runItem(item: TransferItem, policy: { current: ConflictChoice | null }) {
    if (item.kind === "mkdir") {
      try {
        await backend.hostCreateDir(item.to);
        update(item.id, { status: "done" });
      } catch (e) {
        update(item.id, { status: "failed", error: toErrorInfo(e) });
      }
      return;
    }
    if (!deviceId) {
      update(item.id, { status: "failed" });
      return;
    }
    let target = item.to;
    let overwrite: "never" | "replace" = "never";
    let renameTry = 0;
    for (;;) {
      const request: OperationRequest =
        item.kind === "download"
          ? { type: "files.download", path: item.from, dest: target, overwrite }
          : { type: "files.upload", source: item.from, path: target, overwrite };
      let poll: ReturnType<typeof setInterval> | undefined;
      let result;
      try {
        const opId = await backend.startOperation(deviceId, request);
        currentOp.current = opId;
        poll = setInterval(() => {
          backend
            .operationProgress(opId)
            .then((progress) => progress && update(item.id, { progress }))
            .catch(() => undefined);
        }, 120);
        result = await backend.awaitOperation(opId);
      } catch (e) {
        update(item.id, { status: "failed", error: toErrorInfo(e) });
        if (toErrorInfo(e).code === "DEVICE_DISCONNECTED") stop.current = true;
        return;
      } finally {
        if (poll) clearInterval(poll);
        currentOp.current = null;
      }

      if (result.outcome === "success") {
        const d = result.data;
        const info = d && (d.type === "downloaded" || d.type === "written") ? d : null;
        update(item.id, {
          status: "done",
          progress: null,
          bytes: info?.bytes,
          sha256: info?.sha256,
          replaced: info?.replaced,
        });
        return;
      }
      if (result.outcome === "cancelled") {
        update(item.id, { status: "cancelled", progress: null, uncertain: result.uncertain });
        stop.current = true;
        return;
      }
      if (result.error?.code === "ALREADY_EXISTS") {
        const choice = policy.current ?? (await askConflict({ ...item, to: target }));
        setConflict(null);
        if (choice.all) policy.current = choice;
        if (choice.action === "skip") {
          update(item.id, { status: "skipped", progress: null });
          return;
        }
        if (choice.action === "replace") {
          overwrite = "replace"; // явное решение пользователя
          continue;
        }
        // другое имя: `имя (1).ext`, `имя (2).ext` … пока не найдётся свободное
        renameTry++;
        const { stem, ext } = splitName(baseName(item.to));
        const name = `${stem} (${renameTry})${ext}`;
        target = withName(item.to, name);
        update(item.id, { name, to: target });
        if (renameTry > 200) {
          update(item.id, { status: "failed", error: result.error });
          return;
        }
        continue;
      }
      update(item.id, {
        status: "failed",
        progress: null,
        error: result.error,
        uncertain: result.uncertain,
      });
      // Неопределённая запись и отключение — дальше не идём: решает пользователь.
      if (result.uncertain || result.error?.code === "DEVICE_DISCONNECTED") stop.current = true;
      return;
    }
  }

  const enqueue = useCallback(
    async (plan: PlanItem[]) => {
      if (busyRef.current || plan.length === 0) return;
      busyRef.current = true;
      setBusy(true);
      stop.current = false;
      const batch: TransferItem[] = plan.map((p) => ({
        ...p,
        id: seq.current++,
        status: "pending",
      }));
      setItems((all) => [...all, ...batch]);
      const policy = { current: null as ConflictChoice | null };
      try {
        for (const item of batch) {
          if (stop.current) {
            update(item.id, { status: "cancelled" });
            continue;
          }
          update(item.id, { status: "running" });
          await runItem(item, policy);
        }
      } finally {
        busyRef.current = false;
        setBusy(false);
        onFinished();
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [backend, deviceId, onFinished, update],
  );

  const cancel = useCallback(() => {
    stop.current = true;
    setConflict((c) => {
      c?.resolve({ action: "skip", all: false });
      return null;
    });
    if (currentOp.current) void backend.cancelOperation(currentOp.current);
  }, [backend]);

  const clear = useCallback(
    () => setItems((all) => all.filter((i) => i.status === "running" || i.status === "pending")),
    [],
  );

  const resolveConflict = useCallback(
    (c: ConflictChoice) => {
      conflict?.resolve(c);
      setConflict(null);
    },
    [conflict],
  );

  return { items, busy, conflict: conflict?.item ?? null, resolveConflict, enqueue, cancel, clear };
}
