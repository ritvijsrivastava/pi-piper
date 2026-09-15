// Persists "RC was active in session X" across pi's session replacement
// (`/new`, `/resume`, `/fork`, `/reload`). Those tear down and recreate the
// whole extension runtime, so in-memory state (the HubClient, cached
// contexts) cannot follow. The replacement instance instead learns that it
// should reconnect from this small on-disk record, keyed by session file.
//
// Keyed by file rather than a single slot so several concurrent `pi`
// sessions can each run RC without clobbering one another; entries whose
// owning process is gone are pruned on read.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const HANDOFF_DIR = path.join(homedir(), ".pi", "agent", "pi-piper");
const HANDOFF_FILE = path.join(HANDOFF_DIR, "rc-handoff.json");

type HandoffMap = Record<string, number>; // session file -> owning pi pid

function read(): HandoffMap {
  try {
    const parsed: unknown = JSON.parse(readFileSync(HANDOFF_FILE, "utf8"));
    return typeof parsed === "object" && parsed !== null ? (parsed as HandoffMap) : {};
  } catch {
    return {};
  }
}

function write(map: HandoffMap): void {
  mkdirSync(HANDOFF_DIR, { recursive: true });
  writeFileSync(HANDOFF_FILE, JSON.stringify(map, null, 2));
}

function processAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    // EPERM means the process exists but belongs to another user.
    return (err as NodeJS.ErrnoException).code === "EPERM";
  }
}

/** Records that RC is active for `sessionFile` in this process. */
export function markHandoff(sessionFile: string): void {
  const map = read();
  map[sessionFile] = process.pid;
  write(map);
}

/** Whether `sessionFile` has a live handoff entry. Entries whose owning
 * process is gone are pruned as a side effect. */
export function hasHandoff(sessionFile: string): boolean {
  const map = read();
  const pid = map[sessionFile];
  if (pid === undefined) return false;
  if (pid === process.pid || processAlive(pid)) return true;
  delete map[sessionFile];
  try {
    write(map);
  } catch {
    // best-effort pruning only
  }
  return false;
}

/** Removes the handoff entry for `sessionFile`, if present. */
export function dropHandoff(sessionFile: string): void {
  const map = read();
  if (map[sessionFile] === undefined) return;
  delete map[sessionFile];
  try {
    write(map);
  } catch {
    // best-effort; a stale entry only causes an extra reconnect attempt
  }
}
