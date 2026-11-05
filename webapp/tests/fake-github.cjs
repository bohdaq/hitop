// A fake GitHub for tests/github.cjs, as the HITOP Cloud App sees it (sync-server gitsync.rs):
// the install page (it sends the browser straight back with a code), the OAuth code exchange,
// GET /user/installations, installation tokens, GET /installation/repositories, GET /repos/:o/:r
// and the Git Data API (refs, commits, recursive trees, blobs, new blobs/trees/commits, a
// fast-forward-only ref update). In memory; blob ids are Git's own (SHA-1 of "blob <len>\0...").
// The sync-server tests have the same fake in Rust (sync-server/tests/common/ghapp.rs).
const crypto = require('crypto');
const http = require('http');

const sha1 = (b) => crypto.createHash('sha1').update(b).digest('hex');
const blobId = (text) => sha1(Buffer.concat([Buffer.from(`blob ${Buffer.byteLength(text)}\0`), Buffer.from(text)]));

function fakeGithub({ appId, installation, callback }) {
  const blobs = new Map();
  const trees = new Map();
  const commits = new Map();
  const refs = new Map(); // repo → Map(branch → commit)
  let n = 0;
  const putTree = (t) => {
    const id = sha1(JSON.stringify([...t].sort()));
    trees.set(id, new Map(t));
    return id;
  };
  const putCommit = (c) => {
    const id = sha1(`${c.tree}${c.parents.join()}${c.message}${++n}`);
    commits.set(id, c);
    return id;
  };
  const repo = (name, files) => {
    const t = new Map(Object.entries(files).map(([p, text]) => {
      blobs.set(blobId(text), text);
      return [p, blobId(text)];
    }));
    refs.set(name, new Map([['main', putCommit({ tree: putTree(t), parents: [], message: 'initial', author: {} })]]));
  };
  const files = (name) => {
    const c = commits.get(refs.get(name).get('main'));
    return Object.fromEntries([...trees.get(c.tree)].map(([p, id]) => [p, blobs.get(id)]));
  };
  const head = (name) => commits.get(refs.get(name).get('main'));
  const push = (name, changes) => {
    const at = refs.get(name).get('main');
    const t = new Map(trees.get(commits.get(at).tree));
    for (const [p, text] of Object.entries(changes)) {
      if (text === null) t.delete(p);
      else {
        blobs.set(blobId(text), text);
        t.set(p, blobId(text));
      }
    }
    const c = putCommit({ tree: putTree(t), parents: [at], message: "a colleague's change", author: { name: 'bob' } });
    refs.get(name).set('main', c);
    return c;
  };
  const ancestor = (of, c) => {
    const stack = [c];
    while (stack.length) {
      const x = stack.pop();
      if (x === of) return true;
      stack.push(...(commits.get(x)?.parents ?? []));
    }
    return false;
  };
  const token = `ghs_installation_${installation}`;
  const visits = []; // browser pages opened: 'install' or 'authorize'
  const server = http.createServer((req, res) => {
    let raw = '';
    req.on('data', (d) => (raw += d));
    req.on('end', () => {
      const u = new URL(req.url, 'http://x');
      const segs = u.pathname.split('/').filter(Boolean);
      const auth = (req.headers.authorization || '').replace(/^Bearer /, '');
      const json = (status, v) => res.writeHead(status, { 'Content-Type': 'application/json' }).end(JSON.stringify(v));
      const body = (() => {
        try {
          return JSON.parse(raw || 'null');
        } catch {
          return null;
        }
      })();
      // The App's install page: GitHub installs it and sends the browser to the setup URL.
      if (req.method === 'GET' && segs[0] === 'apps' && segs[2] === 'installations') {
        visits.push('install');
        const to = `${callback()}?code=good-code&installation_id=${installation}&setup_action=install&state=${encodeURIComponent(u.searchParams.get('state'))}`;
        return res.writeHead(302, { Location: to }).end();
      }
      if (req.method === 'GET' && u.pathname === '/login/oauth/authorize') {
        visits.push('authorize');
        return res.writeHead(302, { Location: `${callback()}?code=good-code&state=${encodeURIComponent(u.searchParams.get('state'))}` }).end();
      }
      if (req.method === 'POST' && u.pathname === '/login/oauth/access_token') {
        const form = new URLSearchParams(raw);
        return json(200, form.get('code') === 'good-code' ? { access_token: 'ghu_user' } : { error: 'bad_verification_code' });
      }
      if (u.pathname === '/user/installations' && auth === 'ghu_user') return json(200, { installations: [{ id: installation, app_id: Number(appId), account: { login: 'ada' } }] });
      if (req.method === 'POST' && segs[0] === 'app' && segs[3] === 'access_tokens') return json(201, { token, expires_at: '2099-01-01T00:00:00Z' });
      if (auth !== token) return json(401, { message: 'Bad credentials' });
      if (u.pathname === '/installation/repositories') return json(200, { repositories: [...refs.keys()].map((r) => ({ full_name: r, default_branch: 'main', private: true })) });
      if (segs[0] !== 'repos') return json(404, { message: 'Not Found' });
      const name = `${segs[1]}/${segs[2]}`;
      if (!refs.has(name)) return json(404, { message: 'Not Found' });
      if (segs.length === 3) return json(200, { full_name: name, default_branch: 'main', private: true });
      const rest = segs.slice(4);
      const key = `${req.method} ${rest[0]}`;
      if (key === 'GET ref') {
        const c = refs.get(name).get(rest.slice(2).join('/'));
        return c ? json(200, { object: { sha: c } }) : json(404, { message: 'Not Found' });
      }
      if (key === 'GET commits') return json(200, { sha: rest[1], tree: { sha: commits.get(rest[1]).tree } });
      if (key === 'GET trees') return json(200, { truncated: false, tree: [...trees.get(rest[1])].map(([path, sha]) => ({ path, sha, type: 'blob', mode: '100644' })) });
      if (key === 'GET blobs') return json(200, { encoding: 'base64', content: Buffer.from(blobs.get(rest[1])).toString('base64') });
      if (key === 'POST blobs') {
        blobs.set(blobId(body.content), body.content);
        return json(201, { sha: blobId(body.content) });
      }
      if (key === 'POST trees') {
        const t = new Map(trees.get(body.base_tree) ?? []);
        for (const e of body.tree) e.sha === null ? t.delete(e.path) : t.set(e.path, e.sha);
        return json(201, { sha: putTree(t) });
      }
      if (key === 'POST commits') return json(201, { sha: putCommit({ tree: body.tree, parents: body.parents, message: body.message, author: body.author }) });
      if (key === 'PATCH refs') {
        const branch = rest.slice(2).join('/');
        if (!ancestor(refs.get(name).get(branch), body.sha)) return json(422, { message: 'Update is not a fast forward' });
        refs.get(name).set(branch, body.sha);
        return json(200, { object: { sha: body.sha } });
      }
      json(404, { message: 'Not Found' });
    });
  });
  return { server, repo, files, head, push, refs, visits };
}

module.exports = { fakeGithub };
