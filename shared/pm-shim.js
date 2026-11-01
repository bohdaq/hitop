// hitop's Postman script shim (MIT). One file, used as is by every surface:
// the CLI and desktop embed it in hitop-core (cli/crates/hitop-core/pm-shim.js
// is a byte-for-byte copy, checked by a test) and the web app loads it as text.
//
// It defines two functions and nothing else:
//   __hitopPm(api)              the `pm` object, plus the legacy sandbox names
//                               (tests, responseBody, postman, ...), built on
//                               hitop's script API
//   __hitopPmUnsupported(src)   the script lines that use a pm.* call this
//                               shim doesn't provide (the import report)
//
// `api` is what the host gives:
//   vars          variables (collection, environment, ...) as name -> string
//   setVar(k, v)  sets a variable for the rest of the run (v null: unset)
//   assert        hitop's assert object; assert.ok(pass, name, detail) records a test
//   request       { url, method, headers: [[name, value]], body } as sent or about to be sent
//   setUrl, setHeader, removeHeader, setBody   (pre-request scripts only)
//   response      null, or { code, text, headers: {lowercased name: value}, time }
//
// Postman's collection, environment, global and local scopes are one set of
// variables in hitop, so pm.environment, pm.collectionVariables, pm.globals and
// pm.variables all read and write the same values.

// Every pm.* and postman.* path the shim provides. A path used in a script is
// supported when it is listed here or is the start of a listed path.
var __HITOP_PM_SUPPORTED = (function () {
  var scopes = ["environment", "collectionVariables", "variables", "globals"];
  var scopeMethods = ["get", "set", "has", "unset", "toObject", "replaceIn"];
  var list = ["pm.test", "pm.test.skip", "pm.expect"];
  scopes.forEach(function (s) {
    scopeMethods.forEach(function (m) {
      list.push("pm." + s + "." + m);
    });
  });
  ["code", "status", "responseTime", "responseSize", "json", "text",
    "headers.get", "headers.has", "headers.toObject",
    "to.have.status", "to.have.header", "to.have.body", "to.have.jsonBody",
    "to.be.ok", "to.be.success", "to.be.error", "to.be.clientError", "to.be.serverError",
    "to.be.notFound", "to.be.json",
    "to.not.have.status", "to.not.have.header", "to.not.be.ok", "to.not.be.error",
    "to.not.be.clientError", "to.not.be.serverError", "to.not.be.notFound"].forEach(function (p) {
    list.push("pm.response." + p);
  });
  ["url.toString", "method", "headers.get", "headers.has", "headers.add", "headers.upsert",
    "headers.remove", "headers.toObject", "body.raw", "body.mode", "body.update", "body.toString"].forEach(function (p) {
    list.push("pm.request." + p);
  });
  ["setEnvironmentVariable", "getEnvironmentVariable", "clearEnvironmentVariable",
    "setGlobalVariable", "getGlobalVariable", "clearGlobalVariable"].forEach(function (p) {
    list.push("postman." + p);
  });
  return list;
})();

function __hitopPmUnsupported(src) {
  var out = [];
  var supported = function (path) {
    for (var i = 0; i < __HITOP_PM_SUPPORTED.length; i++) {
      var s = __HITOP_PM_SUPPORTED[i];
      if (s === path || s.indexOf(path + ".") === 0) return true;
    }
    return false;
  };
  var lines = String(src || "").split(/\r?\n/);
  for (var n = 0; n < lines.length; n++) {
    var line = lines[n];
    var code = line.replace(/^\s*\/\/.*$/, "");
    var seen = {};
    var re = /(^|[^\w$.])(pm|postman)((?:\s*\.\s*[A-Za-z_$][\w$]*)+)/g;
    var m;
    while ((m = re.exec(code)) !== null) {
      var path = m[2] + m[3].replace(/\s+/g, "");
      if (!supported(path) && !seen[path]) {
        seen[path] = true;
        out.push({ line: n + 1, call: path, text: line.trim() });
      }
    }
    var libs = /(^|[^\w$.])(require\s*\(|CryptoJS\b|cheerio\b|xml2Json\b|tv4\b|_\s*\.\s*[A-Za-z_$])/g;
    while ((m = libs.exec(code)) !== null) {
      var name = m[2].replace(/\s+/g, "");
      name = name === "require(" ? "require()" : name.charAt(0) === "_" ? "lodash (_)" : name;
      if (!seen[name]) {
        seen[name] = true;
        out.push({ line: n + 1, call: name, text: line.trim() });
      }
    }
  }
  return out;
}

function __hitopPm(api) {
  var vars = api.vars || {};
  var updates = {};
  var depth = 0; // > 0 while a pm.test callback runs
  var response = api.response || null;
  var request = api.request || { url: "", method: "GET", headers: [], body: "" };

  var str = function (v) {
    if (v === undefined || v === null) return v;
    return typeof v === "string" ? v : typeof v === "object" ? JSON.stringify(v) : String(v);
  };
  var get = function (k) {
    if (Object.prototype.hasOwnProperty.call(updates, k)) return updates[k] === null ? undefined : updates[k];
    return Object.prototype.hasOwnProperty.call(vars, k) ? vars[k] : undefined;
  };
  var set = function (k, v) {
    k = String(k);
    var value = v === undefined || v === null ? null : str(v);
    updates[k] = value;
    if (api.setVar) api.setVar(k, value);
  };
  var all = function () {
    var out = {};
    Object.keys(vars).forEach(function (k) { out[k] = vars[k]; });
    Object.keys(updates).forEach(function (k) {
      if (updates[k] === null) delete out[k];
      else out[k] = updates[k];
    });
    return out;
  };
  var replaceIn = function (text) {
    return String(text).replace(/\{\{\s*([^{}]+?)\s*\}\}/g, function (whole, name) {
      var v = get(name);
      return v === undefined ? whole : v;
    });
  };
  var scope = {
    get: get,
    set: set,
    has: function (k) { return get(k) !== undefined; },
    unset: function (k) { set(k, null); },
    toObject: all,
    replaceIn: replaceIn,
  };

  // --- assertions -------------------------------------------------------------
  var show = function (v) {
    if (typeof v === "string") return "'" + v + "'";
    if (v === undefined) return "undefined";
    if (typeof v === "function") return "[Function]";
    if (typeof v === "number" && isNaN(v)) return "NaN";
    try { return JSON.stringify(v); } catch (e) { return String(v); }
  };
  var deepEqual = function (a, b) {
    if (a === b) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return a !== a && b !== b;
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    var ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (var i = 0; i < ka.length; i++) {
      if (!Object.prototype.hasOwnProperty.call(b, ka[i]) || !deepEqual(a[ka[i]], b[ka[i]])) return false;
    }
    return true;
  };
  var typeOf = function (v) {
    if (v === null) return "null";
    if (Array.isArray(v)) return "array";
    if (v instanceof RegExp) return "regexp";
    if (v instanceof Date) return "date";
    return typeof v;
  };
  var failure = function (message) {
    var e = new Error(message);
    e.name = "AssertionError";
    return e;
  };

  function Assertion(subject, flags) {
    this._obj = subject;
    this._not = !!(flags && flags.not);
    this._deep = !!(flags && flags.deep);
    this._include = !!(flags && flags.include);
  }
  var A = Assertion.prototype;
  // Records a check: inside pm.test a failure throws (pm.test catches it);
  // outside one each check is its own test result.
  A._check = function (cond, positive, negative) {
    var pass = this._not ? !cond : !!cond;
    var message = this._not ? negative : positive;
    if (depth > 0) {
      if (!pass) throw failure(message);
    } else {
      api.assert.ok(pass, message, pass ? "" : message);
    }
    return this;
  };
  ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "but", "does", "still", "also"].forEach(function (w) {
    Object.defineProperty(A, w, { get: function () { return this; } });
  });
  Object.defineProperty(A, "not", { get: function () { this._not = !this._not; return this; } });
  Object.defineProperty(A, "deep", { get: function () { this._deep = true; return this; } });
  var getterChecks = {
    ok: function () {
      if (this._obj && this._obj.__pmResponse) {
        var c = this._obj.code;
        return this._check(c >= 200 && c < 300, "expected response to have status code 2xx but got " + c, "expected response not to have status code 2xx");
      }
      return this._check(!!this._obj, "expected " + show(this._obj) + " to be truthy", "expected " + show(this._obj) + " to be falsy");
    },
    true: function () { return this._check(this._obj === true, "expected " + show(this._obj) + " to be true", "expected " + show(this._obj) + " to be false"); },
    false: function () { return this._check(this._obj === false, "expected " + show(this._obj) + " to be false", "expected " + show(this._obj) + " to be true"); },
    null: function () { return this._check(this._obj === null, "expected " + show(this._obj) + " to be null", "expected " + show(this._obj) + " not to be null"); },
    undefined: function () { return this._check(this._obj === undefined, "expected " + show(this._obj) + " to be undefined", "expected " + show(this._obj) + " not to be undefined"); },
    NaN: function () { return this._check(typeof this._obj === "number" && isNaN(this._obj), "expected " + show(this._obj) + " to be NaN", "expected " + show(this._obj) + " not to be NaN"); },
    exist: function () { return this._check(this._obj !== null && this._obj !== undefined, "expected " + show(this._obj) + " to exist", "expected " + show(this._obj) + " not to exist"); },
    empty: function () {
      var o = this._obj;
      var empty = typeof o === "string" || Array.isArray(o) ? o.length === 0 : o && typeof o === "object" ? Object.keys(o).length === 0 : false;
      return this._check(empty, "expected " + show(o) + " to be empty", "expected " + show(o) + " not to be empty");
    },
    // Response checks (pm.response.to.be.*, pm.expect(pm.response).to.be.*).
    success: function () { return getterChecks.ok.call(this); },
    error: function () { var c = this._obj && this._obj.code; return this._check(c >= 400, "expected response to have status code 4xx or 5xx but got " + c, "expected response not to be an error but got " + c); },
    clientError: function () { var c = this._obj && this._obj.code; return this._check(c >= 400 && c < 500, "expected response to have status code 4xx but got " + c, "expected response not to have status code 4xx"); },
    serverError: function () { var c = this._obj && this._obj.code; return this._check(c >= 500, "expected response to have status code 5xx but got " + c, "expected response not to have status code 5xx"); },
    notFound: function () { var c = this._obj && this._obj.code; return this._check(c === 404, "expected response to have status code 404 but got " + c, "expected response not to have status code 404"); },
    json: function () {
      var ok = true;
      try { JSON.parse(this._obj.text()); } catch (e) { ok = false; }
      return this._check(ok, "expected response body to be valid JSON", "expected response body not to be JSON");
    },
  };
  Object.keys(getterChecks).forEach(function (k) {
    Object.defineProperty(A, k, { get: getterChecks[k] });
  });
  Object.defineProperty(A, "include", {
    get: function () {
      var self = this;
      var fn = function (v) { return A.include_.call(self, v); };
      self._include = true;
      // `.include.members([...])`, `.include.keys(...)`: chain on.
      ["members", "keys", "key"].forEach(function (k) { fn[k] = function () { return A[k].apply(self, arguments); }; });
      return fn;
    },
  });
  A.contain = A.contains = A.includes = function (v) { return A.include_.call(this, v); };
  A.include_ = function (v) {
    var o = this._obj, found;
    if (typeof o === "string") found = o.indexOf(String(v)) >= 0;
    else if (Array.isArray(o)) found = o.some(function (x) { return x === v || (typeof v === "object" && deepEqual(x, v)); });
    else if (o && typeof o === "object" && v && typeof v === "object") found = Object.keys(v).every(function (k) { return deepEqual(o[k], v[k]); });
    else found = false;
    return this._check(found, "expected " + show(o) + " to include " + show(v), "expected " + show(o) + " not to include " + show(v));
  };
  A.equal = A.equals = A.eq = function (v) {
    var same = this._deep ? deepEqual(this._obj, v) : this._obj === v;
    return this._check(same, "expected " + show(this._obj) + " to " + (this._deep ? "deeply " : "") + "equal " + show(v), "expected " + show(this._obj) + " not to equal " + show(v));
  };
  A.eql = A.eqls = function (v) {
    return this._check(deepEqual(this._obj, v), "expected " + show(this._obj) + " to deeply equal " + show(v), "expected " + show(this._obj) + " not to deeply equal " + show(v));
  };
  var compare = function (names, test, word) {
    names.forEach(function (n) {
      A[n] = function (v) { return this._check(test(this._obj, v), "expected " + show(this._obj) + " to be " + word + " " + show(v), "expected " + show(this._obj) + " not to be " + word + " " + show(v)); };
    });
  };
  compare(["above", "gt", "greaterThan"], function (a, b) { return a > b; }, "above");
  compare(["below", "lt", "lessThan"], function (a, b) { return a < b; }, "below");
  compare(["least", "gte", "greaterThanOrEqual"], function (a, b) { return a >= b; }, "at least");
  compare(["most", "lte", "lessThanOrEqual"], function (a, b) { return a <= b; }, "at most");
  A.within = function (lo, hi) { return this._check(this._obj >= lo && this._obj <= hi, "expected " + show(this._obj) + " to be within " + lo + ".." + hi, "expected " + show(this._obj) + " not to be within " + lo + ".." + hi); };
  A.closeTo = A.approximately = function (v, delta) { return this._check(Math.abs(this._obj - v) <= delta, "expected " + show(this._obj) + " to be close to " + v + " +/- " + delta, "expected " + show(this._obj) + " not to be close to " + v); };
  A.a = A.an = function (type) {
    var t = String(type).toLowerCase();
    return this._check(typeOf(this._obj) === t, "expected " + show(this._obj) + " to be a " + t, "expected " + show(this._obj) + " not to be a " + t);
  };
  A.instanceOf = A.instanceof = function (C) { return this._check(this._obj instanceof C, "expected " + show(this._obj) + " to be an instance of " + (C && C.name), "expected " + show(this._obj) + " not to be an instance of " + (C && C.name)); };
  A.property = function (name, value) {
    var o = this._obj;
    var has = o !== null && o !== undefined && (typeof o === "object" || typeof o === "function" ? name in Object(o) : Object(o)[name] !== undefined);
    if (arguments.length > 1) {
      var same = has && (this._deep ? deepEqual(o[name], value) : o[name] === value);
      this._check(same, "expected " + show(o) + " to have property '" + name + "' of " + show(value) + (has ? ", but got " + show(o[name]) : ""), "expected " + show(o) + " not to have property '" + name + "' of " + show(value));
    } else {
      this._check(has, "expected " + show(o) + " to have property '" + name + "'", "expected " + show(o) + " not to have property '" + name + "'");
    }
    // Like chai, the chain goes on with the property's value.
    if (!this._not) this._obj = has ? o[name] : undefined;
    return this;
  };
  A.ownProperty = A.haveOwnProperty = function (name) {
    var o = this._obj;
    return this._check(o !== null && o !== undefined && Object.prototype.hasOwnProperty.call(o, name), "expected " + show(o) + " to have own property '" + name + "'", "expected " + show(o) + " not to have own property '" + name + "'");
  };
  A.lengthOf = A.length = function (n) {
    var len = this._obj === null || this._obj === undefined ? undefined : this._obj.length;
    return this._check(len === n, "expected " + show(this._obj) + " to have a length of " + n + " but got " + len, "expected " + show(this._obj) + " not to have a length of " + n);
  };
  A.match = A.matches = function (re) { return this._check(re.test(String(this._obj)), "expected " + show(this._obj) + " to match " + re, "expected " + show(this._obj) + " not to match " + re); };
  A.string = function (s) { return this._check(String(this._obj).indexOf(s) >= 0, "expected " + show(this._obj) + " to contain " + show(s), "expected " + show(this._obj) + " not to contain " + show(s)); };
  A.oneOf = function (list) { var o = this._obj; return this._check(list.some(function (x) { return x === o; }), "expected " + show(o) + " to be one of " + show(list), "expected " + show(o) + " not to be one of " + show(list)); };
  A.keys = A.key = function () {
    var want = arguments.length === 1 && typeof arguments[0] === "object" ? (Array.isArray(arguments[0]) ? arguments[0] : Object.keys(arguments[0])) : Array.prototype.slice.call(arguments);
    var o = this._obj || {};
    var have = Object.keys(o);
    var all = want.every(function (k) { return have.indexOf(k) >= 0; });
    var exact = this._include ? all : all && have.length === want.length;
    return this._check(exact, "expected " + show(o) + " to have keys " + show(want), "expected " + show(o) + " not to have keys " + show(want));
  };
  A.members = function (list) {
    var o = this._obj || [];
    var contains = function (arr, x) { return arr.some(function (y) { return deepEqual(x, y); }); };
    var all = list.every(function (x) { return contains(o, x); });
    var same = this._include ? all : all && o.length === list.length && o.every(function (x) { return contains(list, x); });
    return this._check(same, "expected " + show(o) + " to have the same members as " + show(list), "expected " + show(o) + " not to have the same members as " + show(list));
  };
  A.satisfy = A.satisfies = function (fn) { return this._check(fn(this._obj), "expected " + show(this._obj) + " to satisfy the given function", "expected " + show(this._obj) + " not to satisfy the given function"); };
  // Response checks with arguments.
  A.status = function (code) {
    var c = this._obj && this._obj.code;
    if (typeof code === "string") {
      return this._check(this._obj && this._obj.status === code, "expected response to have status reason '" + code + "' but got '" + (this._obj && this._obj.status) + "'", "expected response not to have status reason '" + code + "'");
    }
    return this._check(c === Number(code), "expected response to have status code " + code + " but got " + c, "expected response not to have status code " + code);
  };
  A.header = function (name, value) {
    var h = this._obj && this._obj.headers ? this._obj.headers.get(name) : undefined;
    if (value === undefined) return this._check(h !== undefined, "expected response to have header " + name, "expected response not to have header " + name);
    return this._check(h === String(value), "expected response to have header " + name + " with value " + show(String(value)) + " but got " + show(h), "expected response not to have header " + name + " with value " + show(String(value)));
  };
  A.body = function (text) {
    var b = this._obj && this._obj.text ? this._obj.text() : undefined;
    if (text === undefined) return this._check(b !== undefined && b !== "", "expected response to have a body", "expected response not to have a body");
    if (text instanceof RegExp) return this._check(text.test(b), "expected response body to match " + text, "expected response body not to match " + text);
    return this._check(b === String(text), "expected response body to equal " + show(String(text)), "expected response body not to equal " + show(String(text)));
  };
  A.jsonBody = function (path, value) {
    var body;
    try { body = this._obj.json(); } catch (e) { return this._check(false, "expected response body to be valid JSON", "expected response body not to be JSON"); }
    if (path === undefined) return this._check(true, "", "expected response body not to be JSON");
    if (typeof path === "object") return this._check(deepEqual(body, path), "expected response body to deeply equal " + show(path), "expected response body not to deeply equal " + show(path));
    var v = String(path).split(".").reduce(function (x, k) { return x !== null && typeof x === "object" ? x[k] : undefined; }, body);
    if (arguments.length < 2) return this._check(v !== undefined, "expected response body to have path '" + path + "'", "expected response body not to have path '" + path + "'");
    return this._check(deepEqual(v, value), "expected response body path '" + path + "' to equal " + show(value) + " but got " + show(v), "expected response body path '" + path + "' not to equal " + show(value));
  };

  var expect = function (subject) { return new Assertion(subject); };

  var test = function (name, fn) {
    if (typeof fn !== "function") return;
    depth++;
    try {
      fn();
      api.assert.ok(true, String(name));
    } catch (e) {
      api.assert.ok(false, String(name), e && e.message ? (e.name === "AssertionError" ? e.message : e.name + ": " + e.message) : String(e));
    } finally {
      depth--;
    }
  };
  test.skip = function () {};

  // --- request and response ---------------------------------------------------
  var headerList = function (pairs, mutable) {
    var find = function (n) {
      n = String(n).toLowerCase();
      for (var i = pairs().length - 1; i >= 0; i--) if (String(pairs()[i][0]).toLowerCase() === n) return pairs()[i][1];
      return undefined;
    };
    var list = {
      get: find,
      has: function (n) { return find(n) !== undefined; },
      toObject: function () {
        var out = {};
        pairs().forEach(function (p) { out[p[0]] = p[1]; });
        return out;
      },
    };
    if (mutable) {
      var put = function (h, v) {
        if (typeof h === "string") {
          var i = h.indexOf(":");
          if (v === undefined && i > 0) api.setHeader(h.slice(0, i).trim(), h.slice(i + 1).trim());
          else api.setHeader(h, str(v));
        } else if (h) api.setHeader(h.key, str(h.value));
      };
      list.add = put;
      list.upsert = put;
      list.remove = function (n) { if (api.removeHeader) api.removeHeader(typeof n === "string" ? n : n.key); };
    }
    return list;
  };

  var mutable = !!api.setUrl;
  var pmRequest = { headers: headerList(function () { return request.headers; }, mutable) };
  Object.defineProperty(pmRequest, "method", { get: function () { return request.method; }, enumerable: true });
  Object.defineProperty(pmRequest, "url", {
    enumerable: true,
    get: function () {
      var u = request.url;
      return { toString: function () { return u; }, raw: u };
    },
    set: function (u) { if (mutable) { request.url = String(u); api.setUrl(String(u)); } },
  });
  var body = {
    mode: "raw",
    toString: function () { return request.body; },
    update: function (b) {
      if (!mutable) return;
      var text = b && typeof b === "object" ? (b.raw !== undefined ? b.raw : JSON.stringify(b)) : String(b);
      request.body = text;
      api.setBody(text);
    },
  };
  Object.defineProperty(body, "raw", {
    enumerable: true,
    get: function () { return request.body; },
    set: function (b) { body.update(String(b)); },
  });
  Object.defineProperty(pmRequest, "body", { get: function () { return body; }, enumerable: true });

  var REASONS = { 200: "OK", 201: "Created", 202: "Accepted", 204: "No Content", 301: "Moved Permanently", 302: "Found", 304: "Not Modified",
    400: "Bad Request", 401: "Unauthorized", 403: "Forbidden", 404: "Not Found", 405: "Method Not Allowed", 409: "Conflict",
    422: "Unprocessable Entity", 429: "Too Many Requests", 500: "Internal Server Error", 502: "Bad Gateway", 503: "Service Unavailable" };
  var pmResponse;
  if (response) {
    var lower = response.headers || {};
    var pairs = Object.keys(lower).map(function (k) { return [k, lower[k]]; });
    pmResponse = {
      __pmResponse: true,
      code: response.code,
      status: REASONS[response.code] || "",
      responseTime: response.time,
      responseSize: String(response.text || "").length,
      headers: headerList(function () { return pairs; }, false),
      text: function () { return response.text; },
      json: function () { return JSON.parse(response.text); },
    };
    Object.defineProperty(pmResponse, "to", { get: function () { return new Assertion(pmResponse); } });
  }

  var pm = {
    environment: scope,
    collectionVariables: scope,
    globals: scope,
    variables: scope,
    request: pmRequest,
    response: pmResponse,
    test: test,
    expect: expect,
  };

  // Postman's legacy sandbox (`tests[...] = bool`, `responseBody`, `postman.*`).
  var tests = {};
  var postman = {
    setEnvironmentVariable: set,
    getEnvironmentVariable: get,
    clearEnvironmentVariable: function (k) { set(k, null); },
    setGlobalVariable: set,
    getGlobalVariable: get,
    clearGlobalVariable: function (k) { set(k, null); },
  };
  return {
    pm: pm,
    tests: tests,
    postman: postman,
    environment: all(),
    globals: all(),
    responseBody: response ? response.text : undefined,
    responseCode: response ? { code: response.code, name: REASONS[response.code] || "", detail: "" } : undefined,
    responseTime: response ? response.time : undefined,
    // Call after the script: turns `tests[name] = value` into test results.
    finish: function () {
      Object.keys(tests).forEach(function (k) {
        api.assert.ok(!!tests[k], k, "was " + show(tests[k]));
      });
    },
  };
}
