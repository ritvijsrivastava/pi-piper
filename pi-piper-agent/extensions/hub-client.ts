// WebSocket client for the `/agent` registration protocol (see
// ARCHITECTURE.md).
// Structurally this is the mirror image of pi-piper's Rust
// `rpc::remote_agent::RemoteAgent`: this side owns the actual socket,
// that side owns the broadcast fan-out.

import WebSocket from "ws";
import { resolveAgentToken, resolveHubUrl } from "./token.ts";

export interface RegisterInfo {
  sessionId: string;
  sessionFile?: string;
  sessionName?: string;
  cwd: string;
}

export type CommandHandler = (command: unknown) => Promise<unknown>;
export type ConnectionStatus = "idle" | "connecting" | "connected" | "disconnected" | "error";
export type StatusHandler = (status: ConnectionStatus, detail?: string) => void;

const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000];

/** One outbound connection to the Hub, registering one session and
 * relaying events out / commands in for its lifetime. Reconnects with
 * backoff on drop; the Hub has no persisted state to resync, so a plain
 * re-register is always sufficient. */
export class HubClient {
  private ws: WebSocket | undefined;
  private closed = false;
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  private status: ConnectionStatus = "idle";
  private statusDetail: string | undefined;

  constructor(
    private info: RegisterInfo,
    private onCommand: CommandHandler,
    private onStatus?: StatusHandler,
  ) {}

  /** Current connection status, for `/rc status` to report accurately
   * instead of a static string — see pi-piper-agent's index.ts. */
  getStatus(): { status: ConnectionStatus; detail?: string } {
    return { status: this.status, detail: this.statusDetail };
  }

  private setStatus(status: ConnectionStatus, detail?: string): void {
    this.status = status;
    this.statusDetail = detail;
    this.onStatus?.(status, detail);
  }

  /** Patches display metadata (e.g. a `/name` rename) without
   * reconnecting — sent as a `meta_update` frame (see ARCHITECTURE.md). */
  updateInfo(patch: Partial<Omit<RegisterInfo, "sessionId">>): void {
    this.info = { ...this.info, ...patch };
    this.sendFrame({ type: "meta_update", sessionId: this.info.sessionId, patch });
  }

  connect(): void {
    this.closed = false;
    this.open();
  }

  close(): void {
    this.closed = true;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    try {
      this.ws?.close();
    } catch {
      // best-effort
    }
  }

  sendEvent(event: unknown): void {
    this.sendFrame({ type: "event", sessionId: this.info.sessionId, event });
  }

  private open(): void {
    if (this.closed) return;
    this.setStatus("connecting");

    let token: string;
    try {
      token = resolveAgentToken();
    } catch (err) {
      this.setStatus("error", err instanceof Error ? err.message : String(err));
      this.scheduleReconnect();
      return;
    }

    const url = `${resolveHubUrl()}?token=${encodeURIComponent(token)}`;
    const ws = new WebSocket(url);
    this.ws = ws;

    ws.on("open", () => {
      this.reconnectAttempt = 0;
      this.sendFrame({
        type: "register",
        sessionId: this.info.sessionId,
        sessionFile: this.info.sessionFile,
        sessionName: this.info.sessionName,
        cwd: this.info.cwd,
      });
    });

    ws.on("message", (data) => {
      let frame: any;
      try {
        frame = JSON.parse(data.toString());
      } catch {
        return;
      }
      if (frame?.type === "registered") {
        this.setStatus("connected");
      } else if (frame?.type === "command") {
        void this.handleCommand(frame.id, frame.command);
      }
    });

    ws.on("close", () => {
      this.setStatus("disconnected");
      if (!this.closed) this.scheduleReconnect();
    });

    ws.on("error", (err) => {
      this.setStatus("error", err instanceof Error ? err.message : String(err));
    });
  }

  private async handleCommand(id: string, command: unknown): Promise<void> {
    let response: unknown;
    try {
      response = await this.onCommand(command);
    } catch (err) {
      const type = (command as { type?: string } | undefined)?.type ?? "unknown";
      response = {
        type: "response",
        command: type,
        success: false,
        error: err instanceof Error ? err.message : String(err),
      };
    }
    this.sendFrame({ type: "response", id, response });
  }

  private sendFrame(frame: unknown): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(frame));
    }
  }

  private scheduleReconnect(): void {
    const delay = RECONNECT_DELAYS_MS[Math.min(this.reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
    this.reconnectAttempt++;
    this.reconnectTimer = setTimeout(() => this.open(), delay);
  }
}
