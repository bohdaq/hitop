// A gRPC request tab: the server address, Send or Cancel; the service and
// method pickers (from reflection or .proto files), the JSON message with a
// template from the method's request type, and the log of what came back
// (headers, messages, status and trailers). Rendered by hitop-request
// (request-view.js) for tabs whose kind is "grpc"; the calls live in
// store.grpcTabs. A browser can't make gRPC calls: the web app keeps and
// saves gRPC requests but sends nothing (platform().grpc is null there).
import { html, nothing } from "lit";
import { platform } from "../platform/index.js";
import { icon } from "../icons.js";
import { errorCard } from "./error-card.js";
import { explainError } from "../errors.js";

const clock = (iso) => {
  const d = new Date(iso);
  const pad = (n, w = 2) => String(n).padStart(w, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
};

const ARROW = { sent: "arrow-up", headers: "info", message: "arrow-down", status: "check-circle", error: "warning-circle", system: "info" };
const LABEL = { sent: "Sent", headers: "Headers", message: "Message", status: "Status", error: "Error", system: "" };

const list = (v) => v.split(",").map((x) => x.trim()).filter(Boolean);

// What a browser shows instead of sending.
export function grpcDesktopOnly() {
  return html`<p class="hint grpc-desktop-only" role="note">
    gRPC is available in the desktop app and CLI (<code>hitop grpc</code>). A browser can't make gRPC calls, so this request is kept and saved here but not sent.
  </p>`;
}

// The request line: the method select (gRPC chosen), the server, Send or
// Cancel, Save.
export function grpcLine(view, methodSelect) {
  const t = view.tab;
  const store = view.store;
  const running = store.grpcState(t.id).status === "running";
  const can = !!platform().grpc;
  const go = (e) => {
    e?.preventDefault();
    if (!can) return;
    if (running) store.grpcCancel();
    else store.send();
  };
  return html`
    <form class="request-line" novalidate @submit=${go}>
      <div class="url-bar">
      ${methodSelect}
      ${view.urlField({ label: "Server", placeholder: "localhost:50051, https://api.example.com or ${grpcHost}", onEnter: go })}
      </div>
      <lit-material-button variant=${running ? "outlined" : "filled"} type="submit" ?disabled=${!can}>
        ${icon(running ? "x" : "paper-plane-tilt")} ${running ? "Cancel" : "Send"}
      </lit-material-button>
      <lit-material-icon-button aria-label="Save request" ?disabled=${!t.url} @click=${() => view.app.openDialog("save", view.store.saveTarget())}>
        ${icon("floppy-disk")}
      </lit-material-icon-button>
    </form>
    ${can ? nothing : grpcDesktopOnly()}`;
}

// Message: service and method (pickers once the schema is loaded, text
// fields before), the JSON message and its template.
export function grpcMessage(view) {
  const t = view.tab;
  const store = view.store;
  const g = store.grpcState(t.id);
  const can = !!platform().grpc;
  const services = g.schema?.services ?? [];
  const service = services.find((s) => s.name === t.grpcService);
  const method = service?.methods.find((m) => m.name === t.grpcMethod);
  const pickService = (name) => {
    const s = services.find((x) => x.name === name);
    view.set({ grpcService: name, grpcMethod: s?.methods[0]?.name ?? "" });
  };
  return html`
    <div class="grpc-target">
      ${services.length
        ? html`<lit-material-select class="grpc-service" variant="outlined" label="Service" .value=${t.grpcService || ""} @change=${(e) => pickService(e.target.value)}>
              ${services.map((s) => html`<lit-material-select-option value=${s.name}>${s.name}</lit-material-select-option>`)}
            </lit-material-select>
            <lit-material-select class="grpc-method" variant="outlined" label="RPC method" .value=${t.grpcMethod || ""} @change=${(e) => view.set({ grpcMethod: e.target.value })}>
              ${(service?.methods ?? []).map(
                (m) => html`<lit-material-select-option value=${m.name}>${m.name}${m.serverStreaming || m.clientStreaming ? ` (${m.clientStreaming && m.serverStreaming ? "bidi" : m.clientStreaming ? "client stream" : "server stream"})` : ""}</lit-material-select-option>`,
              )}
            </lit-material-select>`
        : html`<lit-material-text-field class="grpc-service" variant="outlined" label="Service" placeholder="package.Service" .value=${t.grpcService || ""} @input=${(e) => view.set({ grpcService: e.target.value.trim() })}></lit-material-text-field>
            <lit-material-text-field class="grpc-method" variant="outlined" label="RPC method" placeholder="Method" .value=${t.grpcMethod || ""} @input=${(e) => view.set({ grpcMethod: e.target.value.trim() })}></lit-material-text-field>`}
      <lit-material-button variant="outlined" ?disabled=${!can || g.schemaLoading} @click=${() => store.grpcLoadSchema()}>
        ${icon("arrows-clockwise")} ${g.schemaLoading ? "Loading" : services.length ? "Reload schema" : "Load schema"}
      </lit-material-button>
      <lit-material-button variant="text" ?disabled=${!can || !t.grpcService || !t.grpcMethod} @click=${() => store.grpcTemplate()}>${icon("code")} Template</lit-material-button>
    </div>
    ${g.schemaError ? html`<p class="error-text grpc-schema-error" role="alert">${g.schemaError}</p>` : nothing}
    ${g.schema ? html`<p class="hint grpc-schema-source">${services.length} service${services.length === 1 ? "" : "s"} from ${g.schema.source === "protos" ? ".proto files" : `server ${g.schema.source}`}${method ? `. ${method.input} → ${method.serverStreaming ? "stream " : ""}${method.output}` : ""}</p>` : nothing}
    ${method?.clientStreaming ? html`<p class="hint warn" role="note">HITOP calls unary and server-streaming methods; this one streams from the client.</p>` : nothing}
    <hitop-code-editor class="grpc-message" label="Message (JSON)" language="json" placeholder='{"name": "\${user}"}' .value=${t.grpcMessage || ""}
      @input=${(e) => view.set({ grpcMessage: e.target.value })}></hitop-code-editor>
    <p class="hint">The request message in the proto3 JSON mapping; \${variables} are filled in when sent. Empty sends {}.</p>`;
}

// Schema: server reflection or .proto files, and plaintext.
export function grpcSchema(view) {
  const t = view.tab;
  const store = view.store;
  // A tab-only choice until files are typed: the saved request knows only `protos`.
  const files = t.grpcUseProtos ?? (t.grpcProtos || []).length > 0;
  const folder = t.loadedCollectionId != null ? store.folderOf(t.loadedCollectionId) : null;
  if (platform().grpc?.reflectionOnly) {
    return html`<p class="hint grpc-bridge-note" role="note">The web app reads the schema from the server's reflection service (grpc.reflection v1 or v1alpha) and calls through the HITOP gRPC bridge, which keeps nothing: public servers only. For .proto files or a server on localhost, use the desktop app or <code>hitop grpc</code>.</p>
      <label class="switch-row">
        <lit-material-switch aria-label="Plaintext (no TLS)" ?checked=${!!t.grpcPlaintext} @change=${(e) => view.set({ grpcPlaintext: e.target.checked })}></lit-material-switch>
        <span>Plaintext: HTTP/2 without TLS for a host:port server (http:// addresses always are)</span>
      </label>`;
  }
  return html`
    <lit-material-select class="grpc-source" variant="outlined" label="Schema from" .value=${files ? "protos" : "reflection"}
      @change=${(e) => view.set(e.target.value === "protos" ? { grpcUseProtos: true } : { grpcUseProtos: false, grpcProtos: [], grpcImportPaths: [] })}>
      <lit-material-select-option value="reflection">Server reflection</lit-material-select-option>
      <lit-material-select-option value="protos">.proto files</lit-material-select-option>
    </lit-material-select>
    ${files
      ? html`<lit-material-text-field class="grpc-protos" variant="outlined" label=".proto files"
            supporting-text=${`Separated by commas, relative to ${folder ? "the collection folder" : "the folder hitop runs in"} (or absolute).`}
            .value=${(t.grpcProtos || []).join(", ")} @change=${(e) => view.set({ grpcProtos: list(e.target.value) })}></lit-material-text-field>
          <lit-material-text-field class="grpc-imports" variant="outlined" label="Import paths"
            supporting-text="Directories imports are looked up in, besides each file's own. google/protobuf files are built in."
            .value=${(t.grpcImportPaths || []).join(", ")} @change=${(e) => view.set({ grpcImportPaths: list(e.target.value) })}></lit-material-text-field>`
      : html`<p class="hint">The service list and message types come from the server's reflection service (grpc.reflection v1 or v1alpha).</p>`}
    <label class="switch-row">
      <lit-material-switch aria-label="Plaintext (no TLS)" ?checked=${!!t.grpcPlaintext} @change=${(e) => view.set({ grpcPlaintext: e.target.checked })}></lit-material-switch>
      <span>Plaintext: HTTP/2 without TLS for a host:port server (http:// addresses always are)</span>
    </label>`;
}

// Below the metadata editor.
export function grpcMetadataNote() {
  return html`<p class="hint">Sent as gRPC metadata with the call (names in lower case; a name ending in -bin takes a base64 value). Bearer, Basic and header API key auth are sent here too.</p>`;
}

// The log: status, then every event with its time.
export function grpcLog(view) {
  const t = view.tab;
  const store = view.store;
  const g = store.grpcState(t.id);
  const last = [...g.log].reverse().find((e) => e.kind === "status" || e.kind === "error" || e.cancelled);
  const [cls, label] =
    g.status === "running" ? ["info", "Calling"] : !last ? ["info", "Not called"] : last.cancelled ? ["info", "Cancelled"] : last.kind === "error" ? ["bad", "Failed"] : last.ok ? ["ok", "OK"] : ["bad", last.text.split("\n")[0].replace(/\s*\(.*$/, "")];
  // Machine-readable state for tests and agents: idle | running | ok | failed | cancelled.
  const state = g.status === "running" ? "running" : !last ? "idle" : last.cancelled ? "cancelled" : last.kind === "status" && last.ok ? "ok" : "failed";
  const messages = g.log.filter((e) => e.kind === "message").length;
  return html`
    <section class="response ws-log-panel grpc-log-panel" aria-label="Call log">
      <div class="response-meta">
        <span class=${`status-chip ${cls}`} role="status" data-state=${state}>${label}</span>
        <span>${messages} message${messages === 1 ? "" : "s"}</span>
        ${g.log.length ? html`<lit-material-button variant="text" @click=${() => store.grpcClearLog()}>${icon("trash")} Clear</lit-material-button>` : nothing}
      </div>
      ${g.status !== "running" && last && !last.cancelled && (last.kind === "error" || (last.kind === "status" && !last.ok && explainError(last.text, { kind: "grpc" }).known))
        ? errorCard(last.text, "grpc")
        : nothing}
      ${g.log.length
        ? html`<ol class="ws-log grpc-log">
            ${g.log.map(
              (e) => html`<li class=${`ws-event grpc-event ${e.kind}${e.kind === "status" ? (e.ok ? " ok" : " bad") : ""}`}>
                <time datetime=${e.time}>${clock(e.time)}</time>
                <span class="ws-dir" title=${LABEL[e.kind] || "Event"}>${icon(e.kind === "status" && !e.ok ? "warning-circle" : ARROW[e.kind])}<span class="visually-hidden">${LABEL[e.kind]}</span></span>
                <span class="ws-text">${e.kind === "status" ? html`<strong>Status</strong> ` : e.kind === "headers" ? html`<strong>Headers</strong><br />` : nothing}${e.text}</span>
              </li>`,
            )}
          </ol>`
        : html`<lit-material-empty-state size="compact" headline=${platform().grpc ? "Send to see the response" : "gRPC is available in the desktop app and CLI"}>
            The response headers, each message the server sends, and the final status with its trailers appear here, with the time of each.
          </lit-material-empty-state>`}
    </section>`;
}
