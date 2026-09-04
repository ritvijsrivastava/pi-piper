// Piper mobile client.
//
// Deliberately framework-free: this is a small enough UI that a build
// step would add more complexity than it removes (KISS/YAGNI).
//
// Protocol note: this client speaks pi's RPC protocol directly over the
// WebSocket (see pi's docs/rpc.md). Piper's server only relays JSON; it
// does not define a separate wire format, so this file is the closest
// thing to protocol documentation on the client side.

(() => {
  const TOKEN_KEY = "piper.token";

  const statusDot = document.getElementById("status-dot");
  const statusText = document.getElementById("status-text");
  const settingsPanel = document.getElementById("settings");
  const settingsToggle = document.getElementById("settings-toggle");
  const tokenInput = document.getElementById("token-input");
  const settingsSave = document.getElementById("settings-save");
  const transcript = document.getElementById("transcript");
  const messageInput = document.getElementById("message-input");
  const sendButton = document.getElementById("send-button");
  const abortButton = document.getElementById("abort-button");

  /** @type {WebSocket | null} */
  let socket = null;
  let reconnectDelayMs = 1000;
  const MAX_RECONNECT_DELAY_MS = 15000;

  let isStreaming = false;
  /** Bubble currently receiving streamed assistant text, if any. */
  let currentAssistantBubble = null;
  /** Bubble currently receiving streamed thinking text, if any. */
  let currentThinkingBubble = null;
  /** toolCallId -> bubble element, for updating on tool_execution_end. */
  const toolBubbles = new Map();

  // ---- Token / URL handling -------------------------------------------------

  // A token can be dropped straight into the page URL (?token=...) so the
  // page is bookmarkable/shareable on the phone; it's persisted to
  // localStorage immediately and stripped from the visible URL.
  function loadTokenFromQueryString() {
    const params = new URLSearchParams(window.location.search);
    const token = params.get("token");
    if (token) {
      localStorage.setItem(TOKEN_KEY, token);
      params.delete("token");
      const rest = params.toString();
      const newUrl = window.location.pathname + (rest ? `?${rest}` : "");
      window.history.replaceState({}, "", newUrl);
    }
  }

  function getToken() {
    return localStorage.getItem(TOKEN_KEY) || "";
  }

  function websocketUrl() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const token = encodeURIComponent(getToken());
    return `${protocol}//${window.location.host}/ws?token=${token}`;
  }

  // ---- Connection lifecycle --------------------------------------------------

  function setStatus(kind, text) {
    statusDot.className = `dot dot-${kind}`;
    statusText.textContent = text;
  }

  function connect() {
    if (!getToken()) {
      setStatus("disconnected", "No token set");
      settingsPanel.hidden = false;
      return;
    }

    setStatus("connecting", "Connecting\u2026");
    socket = new WebSocket(websocketUrl());

    socket.addEventListener("open", () => {
      reconnectDelayMs = 1000;
      setStatus("connected", "Connected");
      settingsPanel.hidden = true;
      // Hydrate the transcript and current streaming state.
      send({ type: "get_messages" });
      send({ type: "get_state" });
    });

    socket.addEventListener("message", (ev) => {
      let event;
      try {
        event = JSON.parse(ev.data);
      } catch {
        return;
      }
      handleEvent(event);
    });

    socket.addEventListener("close", () => {
      setStatus("disconnected", "Disconnected \u2013 reconnecting\u2026");
      scheduleReconnect();
    });

    socket.addEventListener("error", () => {
      socket?.close();
    });
  }

  function scheduleReconnect() {
    setTimeout(connect, reconnectDelayMs);
    reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS);
  }

  function send(command) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(command));
    }
  }

  // ---- Rendering --------------------------------------------------------------

  function appendBubble(className, text) {
    const bubble = document.createElement("div");
    bubble.className = `bubble ${className}`;
    bubble.textContent = text;
    transcript.appendChild(bubble);
    transcript.scrollTop = transcript.scrollHeight;
    return bubble;
  }

  /** Extracts plain text from a UserMessage/AssistantMessage `content`
   * field, which may be a string or an array of content blocks. */
  function extractText(content) {
    if (typeof content === "string") return content;
    if (!Array.isArray(content)) return "";
    return content
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join("");
  }

  /** Renders one historical message from `get_messages` into the
   * transcript. Live streaming is handled separately by handleEvent. */
  function renderHistoryMessage(message) {
    switch (message.role) {
      case "user":
        appendBubble("bubble-user", extractText(message.content));
        break;
      case "assistant": {
        const text = extractText(message.content);
        if (text) appendBubble("bubble-assistant", text);
        break;
      }
      case "bashExecution":
        appendBubble("bubble-tool", `$ ${message.command}\n${message.output}`);
        break;
      default:
        // Tool results are shown live via tool_execution_end; skip them
        // here to avoid duplicating that detail on every reconnect.
        break;
    }
  }

  function setStreaming(streaming) {
    isStreaming = streaming;
    abortButton.hidden = !streaming;
    sendButton.textContent = streaming ? "Steer" : "Send";
    if (!streaming) {
      currentAssistantBubble = null;
      currentThinkingBubble = null;
    }
  }

  // ---- Event handling -----------------------------------------------------

  function handleEvent(event) {
    switch (event.type) {
      case "response":
        if (event.command === "get_messages" && event.success) {
          transcript.innerHTML = "";
          for (const message of event.data.messages) renderHistoryMessage(message);
        }
        if (event.command === "get_state" && event.success) {
          setStreaming(Boolean(event.data.isStreaming));
        }
        break;

      case "agent_start":
        setStreaming(true);
        break;

      case "agent_settled":
        setStreaming(false);
        break;

      case "message_start":
        if (event.message.role === "user") {
          appendBubble("bubble-user", extractText(event.message.content));
        } else if (event.message.role === "assistant") {
          currentAssistantBubble = null;
          currentThinkingBubble = null;
        }
        break;

      case "message_update":
        handleAssistantDelta(event.assistantMessageEvent);
        break;

      case "message_end":
        if (event.message.role === "assistant") {
          // message_end is authoritative; replace whatever the deltas
          // produced with the final text in case any were missed.
          const text = extractText(event.message.content);
          if (text) {
            if (!currentAssistantBubble) {
              currentAssistantBubble = appendBubble("bubble-assistant", "");
            }
            currentAssistantBubble.textContent = text;
          }
        }
        break;

      case "tool_execution_start": {
        const bubble = appendBubble("bubble-tool", `\u{1F527} ${event.toolName}\u2026`);
        toolBubbles.set(event.toolCallId, bubble);
        break;
      }

      case "tool_execution_end": {
        const bubble = toolBubbles.get(event.toolCallId);
        if (bubble) {
          const icon = event.isError ? "\u274C" : "\u2705";
          const text = (event.result?.content || [])
            .filter((block) => block.type === "text")
            .map((block) => block.text)
            .join("\n");
          bubble.textContent = `${icon} ${event.toolName}\n${text}`.trim();
          toolBubbles.delete(event.toolCallId);
        }
        break;
      }

      default:
        // Other event types (queue_update, compaction_*, extension UI
        // requests, etc.) are intentionally not surfaced in this minimal
        // client yet.
        break;
    }
  }

  function handleAssistantDelta(delta) {
    switch (delta.type) {
      case "text_delta":
        if (!currentAssistantBubble) {
          currentAssistantBubble = appendBubble("bubble-assistant", "");
        }
        currentAssistantBubble.textContent += delta.delta;
        transcript.scrollTop = transcript.scrollHeight;
        break;
      case "thinking_delta":
        if (!currentThinkingBubble) {
          currentThinkingBubble = appendBubble("bubble-thinking", "");
        }
        currentThinkingBubble.textContent += delta.delta;
        transcript.scrollTop = transcript.scrollHeight;
        break;
      default:
        break;
    }
  }

  // ---- Composer -------------------------------------------------------------

  function submitMessage() {
    const text = messageInput.value.trim();
    if (!text) return;

    if (isStreaming) {
      send({ type: "steer", message: text });
    } else {
      send({ type: "prompt", message: text });
    }
    messageInput.value = "";
    autoResizeInput();
  }

  function autoResizeInput() {
    messageInput.style.height = "auto";
    messageInput.style.height = `${messageInput.scrollHeight}px`;
  }

  sendButton.addEventListener("click", submitMessage);
  abortButton.addEventListener("click", () => send({ type: "abort" }));

  messageInput.addEventListener("input", autoResizeInput);
  messageInput.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.shiftKey) {
      ev.preventDefault();
      submitMessage();
    }
  });

  settingsToggle.addEventListener("click", () => {
    settingsPanel.hidden = !settingsPanel.hidden;
  });

  settingsSave.addEventListener("click", () => {
    localStorage.setItem(TOKEN_KEY, tokenInput.value.trim());
    settingsPanel.hidden = true;
    socket?.close();
    connect();
  });

  // ---- Startup ----------------------------------------------------------------

  loadTokenFromQueryString();
  tokenInput.value = getToken();
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.register("/sw.js").catch(() => {
      // Installability is a nice-to-have; ignore registration failures.
    });
  }
  connect();
})();
