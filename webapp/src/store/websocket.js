// The store's WebSocket (a part of Store, src/store.js).
import { WS_LOG_MAX, errorText, wsUrl } from "./shared.js";
import { applyAuth } from "../features.js";
import { interpolateVariables } from "../../../shared/services/variableInterpolation.js";
import { platform } from "../platform/index.js";
import { withPluginValues } from "../plugin-calls.js";

export const WebsocketPart = (Base) =>
  class extends Base {
  socket(tabId = this.tab?.id) {
    return this.sockets.get(tabId) ?? { status: "closed", protocol: null, log: [], conn: null };
  }

  wsLog(tabId, entry) {
    const s = this.socket(tabId);
    const log = [...s.log, { time: new Date().toISOString(), ...entry }].slice(-WS_LOG_MAX);
    this.sockets = new Map(this.sockets).set(tabId, { ...s, log });
    this.emit();
  }

  wsSet(tabId, changes) {
    this.sockets = new Map(this.sockets).set(tabId, { ...this.socket(tabId), ...changes });
    this.emit();
  }

  // Opens the current tab's connection: URL, headers, auth and subprotocols
  // with variables filled in. The browser sends no handshake headers (its
  // WebSocket can't), the desktop does (Rust); the log says which.
  async wsConnect() {
    const tab = this.tab;
    const id = tab.id;
    if (["open", "connecting"].includes(this.socket(id).status)) return;
    // This connection's identity: callbacks of an older (cancelled) one never touch a newer one.
    const token = {};
    this.wsSet(id, { status: "connecting", protocol: null, token });
    const current = () => this.socket(id).token === token;
    let url, headers, protocols;
    try {
      const answers = await this.resolveRefs({ url: tab.url, headers: tab.headers, auth: tab.auth, subprotocols: tab.subprotocols, wsMessages: tab.wsMessages }, tab.loadedCollectionId);
      const variables = withPluginValues(this.variables(tab.loadedCollectionId), answers);
      const fill = (text) => (text ? interpolateVariables(text, variables) : text);
      const authed = applyAuth(tab.auth, { url: fill(tab.url.trim()), headers: (tab.headers || []).filter((h) => h.name && h.enabled !== false).map((h) => ({ name: fill(h.name), value: fill(h.value || "") })), body: "" }, fill);
      url = wsUrl(authed.url);
      headers = authed.headers;
      protocols = (tab.subprotocols || []).map(fill).filter(Boolean);
    } catch (e) {
      if (current()) this.wsSet(id, { status: "closed", token: null });
      this.notify(`Not connected: ${errorText(e)}`);
      return;
    }
    if (!current()) return; // cancelled while references resolved
    const p = platform();
    this.wsLog(id, { kind: "system", text: `Connecting to ${url}${protocols.length ? ` (subprotocols ${protocols.join(", ")})` : ""}` });
    if (headers.length && !(p.websocketHeaders || p.websocketSendsHeaders?.(url)))
      this.wsLog(id, { kind: "system", text: `Not sent: ${headers.map((h) => h.name).join(", ")}. A browser can't set WebSocket handshake headers (auth included); the desktop app and hitop ws send them. An API key in the query works here.` });
    let opened = false;
    const conn = p.websocket(url, { headers, protocols }, {
      onOpen: ({ protocol }) => {
        if (!current()) return;
        opened = true;
        this.wsSet(id, { status: "open", protocol: protocol || null });
        this.wsLog(id, { kind: "system", text: `Connected${protocol ? `, subprotocol ${protocol}` : ""}` });
      },
      onMessage: ({ text, bytes }) => current() && this.wsLog(id, bytes ? { kind: "received", bytes: Array.from(bytes) } : { kind: "received", text }),
      onError: (message) => current() && this.wsLog(id, { kind: "error", text: opened ? message : `Couldn't connect: ${message}` }),
      onClose: ({ code, reason }) => {
        if (!current()) return;
        this.wsSet(id, { status: "closed", conn: null, token: null });
        if (opened || code) this.wsLog(id, { kind: "system", text: `Closed${code ? ` ${code}` : ""}${reason ? ` ${reason}` : ""}` });
      },
    });
    if (current()) this.wsSet(id, { conn });
    else conn?.close();
  }

  wsDisconnect(tabId = this.tab?.id) {
    const s = this.socket(tabId);
    if (s.status === "closed") return;
    s.conn?.close();
    // Cancel while connecting: closed now, and that connection's late callbacks are ignored (token).
    if (s.status === "connecting") this.wsSet(tabId, { status: "closed", conn: null, token: null });
  }

  // Sends text with variables filled in; logs it as sent.
  wsSend(text) {
    const tab = this.tab;
    const s = this.socket(tab.id);
    if (s.status !== "open" || !s.conn) {
      this.notify("Connect first.");
      return false;
    }
    let out;
    try {
      out = interpolateVariables(text ?? "", this.variables(tab.loadedCollectionId));
      s.conn.send(out);
    } catch (e) {
      this.notify(`Not sent: ${errorText(e)}`);
      return false;
    }
    this.wsLog(tab.id, { kind: "sent", text: out });
    return true;
  }

  wsClearLog(tabId = this.tab?.id) {
    this.wsSet(tabId, { log: [] });
  }
  };
