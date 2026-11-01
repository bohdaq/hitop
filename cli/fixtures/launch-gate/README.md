# Launch-gate scenario

A Postman user's first CI run with hitop: a Postman v2.1 collection (`shop.postman_collection.json`,
written for hitop, MIT like the rest of the repo) and two Postman environments, Staging and Local,
each with a secret `password`. The collection logs in, keeps the token with
`pm.environment.set`, and sends it as the collection's Bearer auth to the next requests.

- `tests/launch_gate.rs` (crate `hitop`): import with `hitop import --to`, run each environment
  against httpmock, check the JUnit report.
- `.github/workflows/ci.yml` job `launch-gate`: the same with the container image built from this
  commit, `bohdaq/hitop-action@v1` and `mock_server.py` (one server per environment, on
  127.0.0.1:8781 and :8782, the ports in the environment files).
