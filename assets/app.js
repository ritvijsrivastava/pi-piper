// Piper mobile client.
//
// Deliberately framework-free: this is a small enough UI that a build
// step would add more complexity than it removes (KISS/YAGNI).
//
// Two screens, hash-routed so the phone's back button/gesture behaves
// like a native app (SPEC.md §9.3):
//   #/sessions        - session list ("home"), fed by /api/sessions +
//                        /ws/control (SPEC.md §9.1)
//   #/session/<id>    - chat view for one session, speaking pi's RPC
//                        protocol directly over /ws?session=<id>
//                        (SPEC.md §9.2) - Piper's server only relays
//                        JSON, it does not define a separate wire format.

(() => {
  const TOKEN_KEY = "piper.token";
  const LAST_SESSION_KEY = "piper.lastSession";

  // ---- Shared elements --------------------------------------------------

  const backButton = document.getElementById("back-button");
  const statusDot = document.getElementById("status-dot");
  const statusText = document.getElementById("status-text");
  const settingsPanel = document.getElementById("settings");
  const settingsToggle = document.getElementById("settings-toggle");
  const tokenInput = document.getElementById("token-input");
  const settingsSave = document.getElementById("settings-save");
  const toastContainer = document.getElementById("toast-container");

  const sessionListView = document.getElementById("session-list-view");
  const sessionListEl = document.getElementById("session-list");
  const sessionListEmpty = document.getElementById("session-list-empty");
  const sessionSearch = document.getElementById("session-search");

  const chatView = document.getElementById("chat-view");
  const transcript = document.getElementById("transcript");
  const messageInput = document.getElementById("message-input");
  const sendButton = document.getElementById("send-button");
  const abortButton = document.getElementById("abort-button");
  const autocompletePopover = document.getElementById("autocomplete-popover");

  // ---- Token handling ----------------------------------------------------

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
      const newUrl = window.location.pathname + (rest ? `?${rest}` : "") + window.location.hash;
      window.history.replaceState({}, "", newUrl);
    }
  }

  function getToken() {
    return localStorage.getItem(TOKEN_KEY) || "";
  }

  function wsUrl(path) {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${protocol}//${window.location.host}${path}`;
  }

  function apiUrl(path) {
    return `${window.location.origin}${path}`;
  }

  // ---- Toasts (also used for extension `notify`) -------------------------

  function showToast(message, kind = "info") {
    const toast = document.createElement("div");
    toast.className = `toast toast-${kind}`;
    toast.textContent = message;
    toastContainer.appendChild(toast);
    setTimeout(() => toast.classList.add("toast-visible"), 10);
    setTimeout(() => {
      toast.classList.remove("toast-visible");
      setTimeout(() => toast.remove(), 300);
    }, 4000);
  }

  // ---- Status bar ---------------------------------------------------------

  function setStatus(kind, text) {
    statusDot.className = `dot dot-${kind}`;
    statusText.textContent = text;
  }

  // ---- Router --------------------------------------------------------------

  function currentRoute() {
    const hash = window.location.hash.replace(/^#\/?/, "");
    if (hash.startsWith("session/")) {
      return { view: "chat", sessionId: decodeURIComponent(hash.slice("session/".length)) };
    }
    return { view: "sessions" };
  }

  function navigateToSessions() {
    window.location.hash = "#/sessions";
  }

  function navigateToSession(sessionId) {
    window.location.hash = `#/session/${encodeURIComponent(sessionId)}`;
  }

  function renderRoute() {
    const route = currentRoute();
    if (route.view === "chat") {
      sessionListView.hidden = true;
      chatView.hidden = false;
      backButton.hidden = false;
      Sessions.stopControlChannel();
      Chat.open(route.sessionId);
    } else {
      chatView.hidden = true;
      sessionListView.hidden = false;
      backButton.hidden = true;
      Chat.close();
      Sessions.start();
    }
  }

  window.addEventListener("hashchange", renderRoute);
  backButton.addEventListener("click", navigateToSessions);

  // ---- Settings -------------------------------------------------------------

  settingsToggle.addEventListener("click", () => {
    settingsPanel.hidden = !settingsPanel.hidden;
  });

  settingsSave.addEventListener("click", () => {
    localStorage.setItem(TOKEN_KEY, tokenInput.value.trim());
    settingsPanel.hidden = true;
    renderRoute();
  });

  // ---- Session list screen (SPEC.md §9.1) -----------------------------------

  const Sessions = (() => {
    /** @type {WebSocket | null} */
    let controlSocket = null;
    let reconnectDelayMs = 1000;
    const MAX_RECONNECT_DELAY_MS = 15000;
    let reconnectTimer = null;

    /** sessionId -> session summary (camelCase fields, see SPEC.md §6.3) */
    const sessions = new Map();
    let searchQuery = "";

    // No local token is required up front: a request may also be
    // authorized by the `Tailscale-User-Login` identity header that
    // `tailscale serve` stamps on automatically (see SPEC.md §7.1),
    // which this page cannot see or set itself — it's added at the
    // network layer regardless of what's in localStorage. So always
    // attempt the request first, and only fall back to prompting for a
    // token if the Hub actually says 401.
    async function start() {
      const authorized = await fetchSnapshot();
      if (authorized) {
        connectControlChannel();
      } else {
        setStatus("disconnected", "Unauthorized \u2013 set a token");
        settingsToggle.hidden = false;
        settingsPanel.hidden = false;
        render();
      }
    }

    /** Returns true if the request was authorized (token match or a
     * Tailscale identity header the Hub accepted), false on a definite
     * 401. Network/other errors are treated as transient and don't
     * block trying the control WebSocket too. */
    async function fetchSnapshot() {
      try {
        const response = await fetch(apiUrl(`/api/sessions?token=${encodeURIComponent(getToken())}`));
        if (response.status === 401) return false;
        if (!response.ok) return true;
        // Authorized with no locally-stored token at all — the only way
        // that's possible is the Tailscale identity header (SPEC.md
        // §7.1). There is nothing to configure in that case, so hide the
        // settings gear entirely instead of leaving an "Access token"
        // control dangling that would suggest one is needed.
        if (!getToken()) settingsToggle.hidden = true;
        const list = await response.json();
        sessions.clear();
        for (const session of list) sessions.set(session.sessionId, session);
        render();
        return true;
      } catch {
        // /ws/control will populate the list once it connects; a failed
        // initial fetch is not fatal.
        return true;
      }
    }

    function connectControlChannel() {
      setStatus("connecting", "Connecting\u2026");
      const token = encodeURIComponent(getToken());
      controlSocket = new WebSocket(wsUrl(`/ws/control?token=${token}`));

      controlSocket.addEventListener("open", () => {
        reconnectDelayMs = 1000;
        setStatus("connected", "Connected");
      });

      controlSocket.addEventListener("message", (ev) => {
        let frame;
        try {
          frame = JSON.parse(ev.data);
        } catch {
          return;
        }
        handleControlFrame(frame);
      });

      controlSocket.addEventListener("close", () => {
        setStatus("disconnected", "Disconnected \u2013 reconnecting\u2026");
        scheduleReconnect();
      });

      controlSocket.addEventListener("error", () => {
        controlSocket?.close();
      });
    }

    function scheduleReconnect() {
      if (currentRoute().view !== "sessions") return;
      reconnectTimer = setTimeout(connectControlChannel, reconnectDelayMs);
      reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS);
    }

    function stopControlChannel() {
      if (reconnectTimer) clearTimeout(reconnectTimer);
      controlSocket?.close();
      controlSocket = null;
    }

    function handleControlFrame(frame) {
      switch (frame.type) {
        case "sessions_snapshot":
          sessions.clear();
          for (const session of frame.sessions) sessions.set(session.sessionId, session);
          render();
          break;
        case "session_connected":
        case "session_update":
        case "session_meta":
          sessions.set(frame.session.sessionId, frame.session);
          render();
          break;
        case "session_disconnected": {
          const existing = sessions.get(frame.sessionId);
          if (existing) sessions.set(frame.sessionId, { ...existing, connected: false });
          render();
          break;
        }
        default:
          break;
      }
    }

    function relativeTime(ms) {
      if (!ms) return "";
      const deltaSeconds = Math.max(0, Math.floor((Date.now() - ms) / 1000));
      if (deltaSeconds < 5) return "just now";
      if (deltaSeconds < 60) return `${deltaSeconds}s ago`;
      const minutes = Math.floor(deltaSeconds / 60);
      if (minutes < 60) return `${minutes}m ago`;
      const hours = Math.floor(minutes / 60);
      if (hours < 24) return `${hours}h ago`;
      return `${Math.floor(hours / 24)}d ago`;
    }

    function matchesSearch(session) {
      if (!searchQuery) return true;
      const haystack = `${session.sessionName || ""} ${session.cwd} ${session.lastPreview || ""}`.toLowerCase();
      return haystack.includes(searchQuery);
    }

    function render() {
      const all = Array.from(sessions.values())
        .filter(matchesSearch)
        .sort((a, b) => (b.lastActivityAtMs || b.connectedAtMs) - (a.lastActivityAtMs || a.connectedAtMs));

      sessionListEl.innerHTML = "";
      sessionListEmpty.hidden = all.length > 0;

      for (const session of all) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "session-item";

        const dot = document.createElement("span");
        dot.className = `dot session-dot ${
          !session.connected ? "dot-disconnected" : session.isStreaming ? "dot-connected dot-pulse" : "dot-idle"
        }`;

        const body = document.createElement("span");
        body.className = "session-item-body";

        const title = document.createElement("span");
        title.className = "session-item-title";
        title.textContent = session.sessionName || cwdBasename(session.cwd) || session.sessionId.slice(0, 8);

        const subtitle = document.createElement("span");
        subtitle.className = "session-item-subtitle";
        const time = relativeTime(session.lastActivityAtMs || session.connectedAtMs);
        subtitle.textContent = `${session.cwd}${time ? " \u00b7 " + time : ""}`;

        body.append(title, subtitle);

        if (session.lastPreview) {
          const preview = document.createElement("span");
          preview.className = "session-item-preview";
          preview.textContent = session.lastPreview;
          body.append(preview);
        }

        item.append(dot, body);
        item.addEventListener("click", () => {
          if (!session.connected) {
            showToast("Session not connected \u2013 reopen it with /rc", "warning");
            return;
          }
          navigateToSession(session.sessionId);
        });

        sessionListEl.appendChild(item);
      }
    }

    function cwdBasename(cwd) {
      if (!cwd) return "";
      const parts = cwd.replace(/\/+$/, "").split("/");
      return parts[parts.length - 1] || cwd;
    }

    sessionSearch.addEventListener("input", () => {
      searchQuery = sessionSearch.value.trim().toLowerCase();
      render();
    });

    return { start, stopControlChannel, fetchSnapshot };
  })();

  // ---- Chat screen (SPEC.md §9.2) -------------------------------------------

  const Chat = (() => {
    /** @type {WebSocket | null} */
    let socket = null;
    let reconnectDelayMs = 1000;
    const MAX_RECONNECT_DELAY_MS = 15000;
    let reconnectTimer = null;
    let currentSessionId = null;

    let isStreaming = false;
    let currentAssistantBubble = null;
    let currentThinkingBubble = null;
    const toolBubbles = new Map();

    /** Cached `get_commands` result for slash autocomplete, refreshed once
     * per connection. */
    let commandCache = [];

    function open(sessionId) {
      if (currentSessionId === sessionId && socket) return;
      close();
      currentSessionId = sessionId;
      localStorage.setItem(LAST_SESSION_KEY, sessionId);
      transcript.innerHTML = "";
      connect();
    }

    function close() {
      if (reconnectTimer) clearTimeout(reconnectTimer);
      socket?.close();
      socket = null;
      currentSessionId = null;
      setStreaming(false);
      toolBubbles.clear();
    }

    function connect() {
      // Same reasoning as Sessions.start(): don't require a local token,
      // the Tailscale identity header (if any) travels with the request
      // regardless. The session list screen already gates entry on a
      // successful /api/sessions call, so by the time this runs we're
      // reasonably confident auth works.
      if (!currentSessionId) return;
      setStatus("connecting", "Connecting\u2026");
      const token = encodeURIComponent(getToken());
      const session = encodeURIComponent(currentSessionId);
      socket = new WebSocket(wsUrl(`/ws?token=${token}&session=${session}`));

      socket.addEventListener("open", () => {
        reconnectDelayMs = 1000;
        setStatus("connected", "Connected");
        send({ type: "get_messages" });
        send({ type: "get_state" });
        send({ type: "get_commands" });
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
      if (currentRoute().view !== "chat" || !currentSessionId) return;
      reconnectTimer = setTimeout(connect, reconnectDelayMs);
      reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS);
    }

    function send(command) {
      if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(command));
      }
    }

    // ---- Rendering ----------------------------------------------------------

    function appendBubble(className, text) {
      const bubble = document.createElement("div");
      bubble.className = `bubble ${className}`;
      bubble.textContent = text;
      transcript.appendChild(bubble);
      transcript.scrollTop = transcript.scrollHeight;
      return bubble;
    }

    /** Collapsible tool-call card using native <details>, so expand/collapse
     * needs no extra JS. */
    function appendToolCard(toolCallId, toolName, argsSummary) {
      const details = document.createElement("details");
      details.className = "bubble bubble-tool";
      const summary = document.createElement("summary");
      summary.textContent = `\u{1F527} ${toolName} ${argsSummary}`.trim();
      const body = document.createElement("pre");
      body.className = "tool-body";
      body.textContent = "";
      details.append(summary, body);
      transcript.appendChild(details);
      transcript.scrollTop = transcript.scrollHeight;
      toolBubbles.set(toolCallId, { details, summary, body, toolName });
      return details;
    }

    function extractText(content) {
      if (typeof content === "string") return content;
      if (!Array.isArray(content)) return "";
      return content
        .filter((block) => block.type === "text")
        .map((block) => block.text)
        .join("");
    }

    function argsSummary(args) {
      if (!args || typeof args !== "object") return "";
      const entries = Object.entries(args).slice(0, 2);
      return entries.map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(" ");
    }

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
          // Tool results are shown live via tool_execution_end; skip here
          // to avoid duplicating that detail on every reconnect.
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

    // ---- Event handling -------------------------------------------------------

    function handleEvent(event) {
      switch (event.type) {
        case "response":
          handleResponse(event);
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
            const text = extractText(event.message.content);
            if (text) {
              if (!currentAssistantBubble) currentAssistantBubble = appendBubble("bubble-assistant", "");
              currentAssistantBubble.textContent = text;
            }
          }
          break;
        case "tool_execution_start":
          appendToolCard(event.toolCallId, event.toolName, argsSummary(event.args));
          break;
        case "tool_execution_end": {
          const card = toolBubbles.get(event.toolCallId);
          if (card) {
            const icon = event.isError ? "\u274C" : "\u2705";
            card.summary.textContent = `${icon} ${card.toolName}`;
            card.body.textContent = (event.result?.content || [])
              .filter((block) => block.type === "text")
              .map((block) => block.text)
              .join("\n");
            toolBubbles.delete(event.toolCallId);
          }
          break;
        }
        case "extension_ui_request":
          handleExtensionUiRequest(event);
          break;
        case "compaction_start":
          showToast("Compacting context\u2026", "info");
          break;
        case "compaction_end":
          showToast(event.aborted ? "Compaction aborted" : "Compaction complete", "info");
          break;
        default:
          // queue_update and a handful of retry/error events are not
          // surfaced in this client yet; see piper-agent/README.md and
          // SPEC.md §12 "Known gaps".
          break;
      }
    }

    function handleResponse(event) {
      if (event.command === "get_messages" && event.success) {
        transcript.innerHTML = "";
        for (const message of event.data.messages) renderHistoryMessage(message);
        return;
      }
      if (event.command === "get_state" && event.success) {
        setStreaming(Boolean(event.data.isStreaming));
        return;
      }
      if (event.command === "get_commands" && event.success) {
        commandCache = event.data.commands || [];
        return;
      }
      if (!event.success && event.error) {
        showToast(event.error, "warning");
      }
    }

    function handleAssistantDelta(delta) {
      switch (delta.type) {
        case "text_delta":
          if (!currentAssistantBubble) currentAssistantBubble = appendBubble("bubble-assistant", "");
          currentAssistantBubble.textContent += delta.delta;
          transcript.scrollTop = transcript.scrollHeight;
          break;
        case "thinking_delta":
          if (!currentThinkingBubble) currentThinkingBubble = appendBubble("bubble-thinking", "");
          currentThinkingBubble.textContent += delta.delta;
          transcript.scrollTop = transcript.scrollHeight;
          break;
        default:
          break;
      }
    }

    // ---- Extension UI dialogs (SPEC.md §9.2) ----------------------------------
    // Basic but functional: uses native browser dialogs rather than a
    // custom modal component. Fine for occasional select/confirm/input
    // prompts; see README for nicer-UI as a future improvement.

    function handleExtensionUiRequest(event) {
      switch (event.method) {
        case "notify":
          showToast(event.message, event.notifyType || "info");
          break;
        case "setStatus":
          if (event.statusText) showToast(event.statusText, "info");
          break;
        case "select": {
          const choice = window.prompt(
            `${event.title}\n\nOptions:\n${event.options.map((o, i) => `${i + 1}. ${o}`).join("\n")}\n\nEnter a number:`,
          );
          const index = choice ? parseInt(choice, 10) - 1 : -1;
          const value = event.options[index];
          send({ type: "extension_ui_response", id: event.id, ...(value ? { value } : { cancelled: true }) });
          break;
        }
        case "confirm": {
          const confirmed = window.confirm(`${event.title}\n\n${event.message}`);
          send({ type: "extension_ui_response", id: event.id, confirmed });
          break;
        }
        case "input": {
          const value = window.prompt(event.title, event.placeholder || "");
          send({
            type: "extension_ui_response",
            id: event.id,
            ...(value !== null ? { value } : { cancelled: true }),
          });
          break;
        }
        default:
          // setWidget/setTitle/set_editor_text/editor: not surfaced in
          // this client yet.
          break;
      }
    }

    // ---- Composer + slash autocomplete ----------------------------------------

    function submitMessage() {
      const text = messageInput.value.trim();
      if (!text) return;
      hideAutocomplete();

      if (isStreaming) {
        send({ type: "prompt", message: text, streamingBehavior: "steer" });
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

    function hideAutocomplete() {
      autocompletePopover.hidden = true;
      autocompletePopover.innerHTML = "";
    }

    function updateAutocomplete() {
      const text = messageInput.value;
      const match = /(?:^|\s)\/([a-zA-Z0-9_:-]*)$/.exec(text);
      if (!match) {
        hideAutocomplete();
        return;
      }
      const prefix = match[1].toLowerCase();
      const matches = commandCache.filter((c) => c.name.toLowerCase().startsWith(prefix)).slice(0, 8);
      if (matches.length === 0) {
        hideAutocomplete();
        return;
      }
      autocompletePopover.innerHTML = "";
      for (const command of matches) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "autocomplete-item";
        const name = document.createElement("span");
        name.className = "autocomplete-name";
        name.textContent = `/${command.name}`;
        item.appendChild(name);
        if (command.description) {
          const desc = document.createElement("span");
          desc.className = "autocomplete-desc";
          desc.textContent = command.description;
          item.appendChild(desc);
        }
        item.addEventListener("click", () => {
          messageInput.value = messageInput.value.replace(/\/[a-zA-Z0-9_:-]*$/, `/${command.name} `);
          messageInput.focus();
          hideAutocomplete();
        });
        autocompletePopover.appendChild(item);
      }
      autocompletePopover.hidden = false;
    }

    sendButton.addEventListener("click", submitMessage);
    abortButton.addEventListener("click", () => send({ type: "abort" }));

    messageInput.addEventListener("input", () => {
      autoResizeInput();
      updateAutocomplete();
    });
    messageInput.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.shiftKey) {
        ev.preventDefault();
        submitMessage();
      }
      if (ev.key === "Escape") hideAutocomplete();
    });

    return { open, close };
  })();

  // ---- Startup ----------------------------------------------------------------

  loadTokenFromQueryString();
  tokenInput.value = getToken();
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.register("/sw.js").then((registration) => {
      // Proactively check for a newer sw.js on every load rather than
      // waiting on the browser's own (much lazier) update heuristic —
      // see sw.js for why staleness here previously hid app fixes.
      registration.update().catch(() => {});
    }).catch(() => {
      // Installability is a nice-to-have; ignore registration failures.
    });
  }
  if (!window.location.hash) {
    window.location.hash = "#/sessions";
  }
  renderRoute();
  // A deep link straight into the chat view (#/session/<id>) never runs
  // Sessions.start(), which is otherwise what decides whether the
  // settings gear should be hidden (authenticated via the Tailscale
  // identity header, see fetchSnapshot above) — so check once here too.
  if (currentRoute().view === "chat") {
    Sessions.fetchSnapshot();
  }
})();
