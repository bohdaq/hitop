// The OAuth 2.0 authorization code grant's way back to the web app (store.js oauthBearer): the
// provider sends the browser here; the code (or the error) and state go to the page that opened
// this window, on this origin only, and the window closes.
(function () {
  const q = new URLSearchParams(location.search);
  const message = { hitopOAuth: { code: q.get("code"), state: q.get("state"), error: q.get("error_description") || q.get("error") } };
  const status = document.getElementById("status");
  if (window.opener) {
    window.opener.postMessage(message, location.origin);
    status.textContent = "Signed in: this window closes.";
    setTimeout(() => window.close(), 300);
  } else {
    status.textContent = "Signed in. Go back to HITOP and send the request again.";
  }
})();
