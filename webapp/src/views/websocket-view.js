// A WebSocket request tab: connect and disconnect, saved and typed
// messages, and the timestamped log of what was sent, received and what
// happened. Rendered by hitop-request (request-view.js) for tabs whose kind
// is "websocket"; the connection lives in store.sockets.
import { html, nothing } from "lit";
import { platform } from "../platform/index.js";
import { icon } from "../icons.js";

const STATUS = {
  open: ["ok", "Connected"],
  connecting: ["info", "Connecting"],
  closed: ["bad", "Not connected"],
};

const clock = (iso) => {
  const d = new Date(iso);
  const pad = (n, w = 2) => String(n).padStart(w, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
};

const hex = (bytes) => bytes.map((b) => b.toString(16).padStart(2, "0")).join(" ");
const base64 = (bytes) => btoa(String.fromCharCode(...bytes));

const ARROW = { sent: "arrow-up", received: "arrow-down", system: "info", error: "warning-circle" };
const LABEL = { sent: "Sent", received: "Received", system: "", error: "Error" };

// The request line: the method select (WebSocket chosen), URL, Connect or
// Disconnect, Save.
export function socketLine(view, methodSelect) {
  const t = view.tab;
  const s = view.store.socket(t.id);
  const busy = s.status !== "closed";
  const toggle = (e) => {
    e?.preventDefault();
    if (busy) view.store.wsDisconnect();
    else view.store.send();
  };
  return html`
    <form class="request-line" novalidate @submit=${toggle}>
      ${methodSelect}
      <lit-material-text-field
        class="url"
        variant="outlined"
        label="URL"
        placeholder="wss://stream.example.com/feed or \${wsBase}/feed"
        .value=${t.url}
        @input=${(e) => view.set({ url: e.target.value })}
        @keydown=${(e) => e.key === "Enter" && toggle(e)}
      ></lit-material-text-field>
      <lit-material-button variant=${busy ? "outlined" : "filled"} type="submit">
        ${icon(busy ? "x" : "plugs-connected")} ${s.status === "open" ? "Disconnect" : s.status === "connecting" ? "Cancel" : "Connect"}
      </lit-material-button>
      <lit-material-icon-button aria-label="Save request" ?disabled=${!t.url} @click=${() => view.app.openDialog("save", view.store.saveTarget())}>
        ${icon("floppy-disk")}
      </lit-material-icon-button>
    </form>`;
}

// Below the headers editor: subprotocols, and what a browser leaves out.
export function socketHeaders(view) {
  const t = view.tab;
  return html`
    <lit-material-text-field
      class="subprotocols"
      variant="outlined"
      label="Subprotocols"
      supporting-text="Offered in Sec-WebSocket-Protocol, in order, separated by commas. \${variables} work."
      .value=${(t.subprotocols || []).join(", ")}
      @change=${(e) => view.set({ subprotocols: e.target.value.split(",").map((x) => x.trim()).filter(Boolean) })}
    ></lit-material-text-field>
    ${platform().websocketHeaders
      ? html`<p class="hint">Headers and auth go with the opening handshake.</p>`
      : html`<p class="hint ws-browser-note" role="note">
          A browser can't set WebSocket handshake headers, so these headers and Bearer, Basic or header API key auth are not sent from here.
          The desktop app and <code>hitop ws</code> send them; an API key in the query works in the browser too.
        </p>`}`;
}

// Messages: the one being typed, and the saved ones with a name each.
export function socketMessages(view) {
  const t = view.tab;
  const store = view.store;
  const open = store.socket(t.id).status === "open";
  const saved = t.wsMessages || [];
  const setSaved = (next) => view.set({ wsMessages: next });
  const draft = t.wsDraft || "";
  return html`
    <div class="ws-compose">
      <hitop-code-editor class="ws-draft" label="Message" language=${/^\s*[[{]/.test(draft) ? "json" : "text"} placeholder='{"op": "subscribe"}' .value=${draft}
        @input=${(e) => view.set({ wsDraft: e.target.value })}></hitop-code-editor>
      <div class="ws-compose-actions">
        <lit-material-button variant="filled" ?disabled=${!open} @click=${() => store.wsSend(draft)}>${icon("paper-plane-tilt")} Send</lit-material-button>
        <lit-material-button variant="text" ?disabled=${!draft} @click=${() => setSaved([...saved, { name: `Message ${saved.length + 1}`, content: draft }])}>
          ${icon("bookmark-simple")} Save as message
        </lit-material-button>
      </div>
    </div>
    ${saved.length
      ? html`<ul class="ws-saved" aria-label="Saved messages">
          ${saved.map((m, i) => {
            const update = (changes) => setSaved(saved.map((x, j) => (j === i ? { ...x, ...changes } : x)));
            return html`<li class="ws-saved-row">
              <lit-material-text-field variant="outlined" label="Name" .value=${m.name || ""} @input=${(e) => update({ name: e.target.value })}></lit-material-text-field>
              <lit-material-text-field variant="outlined" label="Content" .value=${m.content || ""} @input=${(e) => update({ content: e.target.value })}></lit-material-text-field>
              <lit-material-button variant="outlined" aria-label=${`Send ${m.name || "message"}`} ?disabled=${!open} @click=${() => store.wsSend(m.content)}>${icon("paper-plane-tilt")} Send</lit-material-button>
              <lit-material-icon-button aria-label=${`Remove ${m.name || "message"}`} @click=${() => setSaved(saved.filter((_, j) => j !== i))}>${icon("trash")}</lit-material-icon-button>
            </li>`;
          })}
        </ul>`
      : nothing}
    <p class="hint">Text messages; \${variables} are filled in when sent. Saved messages are kept with the request.</p>`;
}

// The log: status, then every event with its time.
export function socketLog(view) {
  const t = view.tab;
  const s = view.store.socket(t.id);
  const [cls, label] = STATUS[s.status];
  return html`
    <section class="response ws-log-panel" aria-label="Messages log">
      <div class="response-meta">
        <span class=${`status-chip ${cls}`} role="status">${label}</span>
        ${s.protocol ? html`<span>Subprotocol ${s.protocol}</span>` : nothing}
        <span>${s.log.length} event${s.log.length === 1 ? "" : "s"}</span>
        ${s.log.length ? html`<lit-material-button variant="text" @click=${() => view.store.wsClearLog()}>${icon("trash")} Clear</lit-material-button>` : nothing}
      </div>
      ${s.log.length
        ? html`<ol class="ws-log">
            ${s.log.map(
              (e) => html`<li class=${`ws-event ${e.kind}`}>
                <time datetime=${e.time}>${clock(e.time)}</time>
                <span class="ws-dir" title=${LABEL[e.kind] || "Event"}>${icon(ARROW[e.kind])}<span class="visually-hidden">${LABEL[e.kind]}</span></span>
                ${e.bytes
                  ? html`<span class="ws-text"><strong>Binary, ${e.bytes.length} bytes</strong><br />hex ${hex(e.bytes)}<br />base64 ${base64(e.bytes)}</span>`
                  : html`<span class="ws-text">${e.text}</span>`}
              </li>`,
            )}
          </ol>`
        : html`<lit-material-empty-state size="compact" headline="Connect to see messages">
            What you send, what the server sends and when the connection opens or closes appear here, with the time of each.
          </lit-material-empty-state>`}
    </section>`;
}
