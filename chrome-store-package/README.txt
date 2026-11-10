HITOP Chrome Extension

The API client your terminal, your CI and your AI agent can all drive.

Since manifest 5.0.0 this extension is the HITOP web app (webapp/, npm run build:extension,
packaged by ./build-chrome-store.sh). The store listing text, permission and data-use answers
are in launch/store-listings.md (the owner uploads; agents never do).

Requests to public hosts go through the HITOP proxy at hitop.rws8.tech; localhost and private
networks go straight from the browser. Scripts run in the manifest's sandboxed page.

Not in the extension (desktop app or the hitop CLI): secret-manager references, the Git panel.
Store reviews delay updates.

https://hitop.rws8.tech/
