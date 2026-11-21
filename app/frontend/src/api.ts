import type { Snapshot } from "./types";
declare global {
  interface Window {
    ipc?: { postMessage: (message: string) => void };
    __paddockReply?: (reply: {
      id: number;
      result: Snapshot | null;
      error: string | null;
    }) => void;
  }
}
let sequence = 0;
const pending = new Map<
  number,
  {
    resolve: (result: Snapshot) => void;
    reject: (error: Error) => void;
    timer: ReturnType<typeof setTimeout>;
  }
>();
window.__paddockReply = (reply) => {
  const entry = pending.get(reply.id);
  if (!entry) return;
  clearTimeout(entry.timer);
  pending.delete(reply.id);
  if (reply.error) entry.reject(new Error(reply.error));
  else if (reply.result) entry.resolve(reply.result);
  else entry.reject(new Error("Empty response from Paddock"));
};
export function invoke(command: string, args: unknown = {}): Promise<Snapshot> {
  return new Promise((resolve, reject) => {
    if (!window.ipc) {
      reject(new Error("Open this interface in the Paddock desktop app."));
      return;
    }
    const id = ++sequence;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error("Paddock did not respond. Try again."));
    }, 120000);
    pending.set(id, { resolve, reject, timer });
    window.ipc.postMessage(JSON.stringify({ id, command, args }));
  });
}
