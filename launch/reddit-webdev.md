# r/webdev (draft, not posted)

**Title:** I made a free API client that imports your Postman collections into plain files you can commit

**Body:**

If you test APIs with Postman and want the collections next to your code instead of in a cloud
workspace, this might help. hitop imports a Postman collection, its environments and globals into
a folder: one YAML file per request, environments in their own files, secret values kept out of
the files (OS keychain, or environment variables in CI).

Your test scripts keep working: `pm.test`, `pm.expect`, `pm.response.json()`, `pm.environment.set`
and friends run as they are. The import tells you exactly which lines it can't run (for example
`pm.sendRequest`).

Then `hitop run ./my-api --env staging` runs it in the terminal or in CI, with a JUnit report and a
GitHub Action.

There's also a web app at https://hitop.rws8.tech/app/ that needs no install or account; it sends
requests to public APIs through a small proxy, so APIs without CORS headers still work.

Free and MIT licensed. Guide: https://hitop.rws8.tech/terms/migrate-from-postman.html

I'd love to hear which of your Postman scripts don't make it across.
