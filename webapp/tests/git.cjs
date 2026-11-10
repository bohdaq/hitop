// The desktop's Git panel (ROADMAP T20) on real repositories: a working
// copy of cli/fixtures/sample with a local bare remote and a second clone
// ("other", a teammate, driven with the git CLI). In Tauri mode the mocked
// `git` command runs hitop-git itself (tauri-mock.cjs, example git-json), so
// status, diff, stage, commit, push, pull, branches and conflicts go through
// libgit2 as in the app. In web mode the panel isn't there.
//   HITOP_MODE=tauri node tests/git.cjs   (after npm run build:tauri and the git-json build)
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');
const { launch, checker } = require('./harness.cjs');
const { listTree } = require('./tauri-mock.cjs');

const { results, check } = checker();
const settle = (p) => p.waitForTimeout(500);
const USER = 'requests/get-user.yaml';

// The git CLI for the test's own setup, without the developer's config.
const GIT_ENV = { ...process.env, GIT_CONFIG_GLOBAL: os.devNull, GIT_CONFIG_NOSYSTEM: '1', GIT_AUTHOR_DATE: '', GIT_COMMITTER_DATE: '' };
const git = (cwd, ...args) => execFileSync('git', args, { cwd, env: GIT_ENV, encoding: 'utf8' }).trim();

function repos() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-git-e2e-'));
  const remote = path.join(root, 'remote.git');
  const work = path.join(root, 'shop');
  git(root, 'init', '-q', '--bare', '-b', 'main', remote);
  fs.mkdirSync(work);
  for (const [rel, text] of Object.entries(listTree(path.join(__dirname, '../../cli/fixtures/sample')))) {
    fs.mkdirSync(path.dirname(path.join(work, rel)), { recursive: true });
    fs.writeFileSync(path.join(work, rel), text);
  }
  fs.writeFileSync(path.join(work, 'README.md'), 'Not a collection file: must survive saves.\n');
  git(work, 'init', '-q', '-b', 'main');
  git(work, 'config', 'user.name', 'Ada Lovelace');
  git(work, 'config', 'user.email', 'ada@example.test');
  git(work, 'remote', 'add', 'origin', remote);
  git(work, 'add', '-A');
  git(work, 'commit', '-q', '-m', 'Sample API');
  git(work, 'push', '-q', '-u', 'origin', 'main');
  const other = path.join(root, 'other');
  git(root, 'clone', '-q', remote, other);
  git(other, 'config', 'user.name', 'Bob');
  git(other, 'config', 'user.email', 'bob@example.test');
  return { root, remote, work, other };
}

// The teammate changes get-user's URL and pushes.
function otherPushesUrl(r, url) {
  git(r.other, 'pull', '-q', '--no-rebase');
  const p = path.join(r.other, USER);
  fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace(/^url: .*$/m, `url: ${url}`));
  git(r.other, 'commit', '-q', '-am', `url ${url}`);
  git(r.other, 'push', '-q');
}

(async () => {
  const h = await launch({ viewport: { width: 1280, height: 1000 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const footer = p.locator('aside .sidebar-footer');

  if (!h.tauri) {
    await footer.getByRole('button', { name: 'Environments' }).waitFor();
    check('[web] no Git page in the browser', (await footer.getByRole('button', { name: 'Git', exact: true }).count()) === 0, await footer.innerText());
    check('no page errors', errors.length === 0, errors.join(' || '));
    return h.finish();
  }

  const r = repos();
  const notes = [];
  await p.exposeFunction('__note', (m) => notes.push(m));
  await p.evaluate(() => document.querySelector('hitop-app').store.addEventListener('notify', (e) => window.__note(e.detail)));
  const lastNote = async (re) => {
    for (let i = 0; i < 60 && !notes.some((n) => re.test(n)); i++) await p.waitForTimeout(100);
    return notes.find((n) => re.test(n)) ?? notes.at(-1) ?? '';
  };
  const panel = p.locator('hitop-git');
  const section = (name) => panel.getByRole('region', { name, exact: true });
  const gitPage = async () => {
    await footer.getByRole('button', { name: 'Git', exact: true }).click();
    await panel.locator('.git-bar').waitFor({ timeout: 10000 });
  };
  const refresh = async () => {
    await panel.getByRole('button', { name: 'Refresh' }).click();
    await settle(p);
  };
  const bar = () => panel.locator('.git-bar').innerText();
  const btn = (label) => panel.locator(`lit-material-button[aria-label="${label}"]`);
  const editUrl = async (url) => {
    await p.locator('aside').getByRole('button', { name: /^GET Get user/ }).click();
    await p.getByLabel('URL').fill(url);
    await p.getByRole('button', { name: 'Save request' }).click();
    await p.waitForTimeout(300);
    await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
    await settle(p);
  };
  const commitAll = async (message) => {
    await section('Changes').getByRole('button', { name: 'Stage all' }).click();
    await settle(p);
    await panel.locator('#git-message').evaluate((el, m) => {
      el.value = m;
      el.dispatchEvent(new Event('input', { bubbles: true }));
    }, message);
    await panel.getByRole('button', { name: /^Commit/ }).click();
    await settle(p);
  };
  const confirmDialog = async (label) => {
    await p.getByRole('button', { name: label, exact: true }).last().click();
    await settle(p);
  };

  // Open the folder; the collection page links to the panel.
  await footer.getByRole('button', { name: 'Git', exact: true }).click();
  check('Git page without a folder says what to do', (await panel.locator('lit-material-empty-state[headline="No collection folder open"]').count()) === 1);
  await p.locator('aside').getByRole('button', { name: 'Collections', exact: true }).click();
  h.mock.nextFolder = r.work;
  await p.getByRole('button', { name: 'Open folder' }).click();
  await p.locator('h1', { hasText: 'Sample API' }).waitFor({ timeout: 10000 });
  await p.locator('.folder-status').getByRole('button', { name: 'Git' }).click();
  await panel.locator('.git-bar').waitFor({ timeout: 10000 });
  const b = await bar();
  check('status: branch, upstream, ahead/behind', b.includes('main') && b.includes('origin/main') && b.includes('↑0 ↓0'), b);
  check('status: clean folder', (await section('Changes').innerText()).includes('No changes'));
  check('commits as git config says', (await section('Commit').innerText()).includes('Ada Lovelace <ada@example.test>'));

  // Edit in the app (live save), then the diff, stage and commit.
  await editUrl('${base}/users/1/profile?fields=name,email');
  await gitPage();
  const changes = await section('Changes').innerText();
  check('an edited request shows as modified', changes.includes(USER) && changes.includes('M'), changes);
  // The shell (HITOP Next): the branch pill in the title bar, the git letter on the tree row.
  const pill = p.locator('header.topbar .branch-pill');
  await pill.waitFor({ timeout: 10000 }).catch(() => {});
  check('title bar: a branch pill with the branch, which opens the Git page', (await pill.count()) === 1 && (await pill.innerText()).includes('main') && (await pill.getAttribute('aria-label')).includes('Open the Git page'), await pill.innerText().catch(() => 'none'));
  await p.waitForFunction(() => [...document.querySelectorAll('aside .git-letter')].some((el) => el.textContent.trim().startsWith('M')), null, { timeout: 10000 }).catch(() => {});
  const letters = await p.locator('aside .git-letter').allInnerTexts();
  check('sidebar: the edited request carries an M', letters.some((l) => l.trim().startsWith('M')), letters.join());
  await section('Changes').getByRole('button', { name: USER }).click();
  const diff = await panel.getByRole('region', { name: `Diff of ${USER}` }).innerText().catch(() => '');
  check('diff shows the changed URL line', diff.includes('-url: ${base}/users/1') && diff.includes('+url: ${base}/users/1/profile'), diff);
  await btn(`Stage ${USER}`).click();
  await settle(p);
  check('stage moves it to Staged', (await section('Staged changes').innerText()).includes(USER));
  await btn(`Unstage ${USER}`).click();
  await settle(p);
  check('unstage moves it back', (await section('Changes').innerText()).includes(USER) && (await section('Staged changes').innerText()).includes('Nothing staged'));
  await commitAll('Profile URL');
  check('commit made, as the configured author', git(r.work, 'log', '-1', '--format=%an <%ae>|%s') === 'Ada Lovelace <ada@example.test>|Profile URL', git(r.work, 'log', '-1', '--format=%an <%ae>|%s'));
  check('README (not a collection file) survives the save', fs.existsSync(path.join(r.work, 'README.md')) && git(r.work, 'status', '--porcelain') === '', git(r.work, 'status', '--porcelain'));
  check('ahead 1 after the commit', (await bar()).includes('↑1 ↓0'), await bar());

  // Push.
  await panel.getByRole('button', { name: /Push/ }).click();
  check('push says where', /Pushed main to origin/.test(await lastNote(/Pushed/)), notes.join(' | '));
  check('the remote has the commit', git(r.remote, 'log', '-1', '--format=%s', 'main') === 'Profile URL');
  await settle(p);
  check('ahead 0 after the push', (await bar()).includes('↑0 ↓0'), await bar());

  // Branches: create and switch, switch back.
  await panel.locator('#git-new-branch').evaluate((el) => (el.value = 'feature'));
  await panel.getByRole('button', { name: 'Create and switch' }).click();
  await settle(p);
  check('new branch is current', (await panel.locator('.git-current').innerText()) === 'feature' && git(r.work, 'branch', '--show-current') === 'feature');
  await btn('Switch to main').click();
  await settle(p);
  check('switched back to main', (await panel.locator('.git-current').innerText()) === 'main' && git(r.work, 'branch', '--show-current') === 'main');

  // Pull: a fast-forward reloads the collection from the folder.
  git(r.other, 'pull', '-q', '--no-rebase');
  const hitopYaml = path.join(r.other, 'hitop.yaml');
  fs.writeFileSync(hitopYaml, fs.readFileSync(hitopYaml, 'utf8').replace(/^name: .*$/m, 'name: Sample API (team)'));
  git(r.other, 'commit', '-q', '-am', 'Rename');
  git(r.other, 'push', '-q');
  await panel.getByRole('button', { name: 'Fetch' }).click();
  await settle(p);
  check('fetch shows behind 1', (await bar()).includes('↑0 ↓1'), await bar());
  await panel.getByRole('button', { name: /Pull/ }).click();
  check('pull fast-forwards', /Pull: fast-forwarded/.test(await lastNote(/Pull:/)), notes.join(' | '));
  await settle(p);
  check('the pulled rename shows in the app', (await p.locator('aside').innerText()).includes('Sample API (team)'));

  // Conflict 1: both change get-user's URL; take theirs.
  otherPushesUrl(r, '${base}/v2/users/1');
  await editUrl('${base}/users/1/mine?fields=name,email');
  await gitPage();
  await commitAll('Mine');
  await panel.getByRole('button', { name: /Pull/ }).click();
  check('pull stops with a conflict', /Pull stopped with 1 conflict/.test(await lastNote(/Pull stopped/)), notes.join(' | '));
  await settle(p);
  check('merge banner and conflict listed', (await panel.locator('.git-merging').innerText()).includes('1 conflict') && (await section('Conflicts').innerText()).includes(USER));
  check('commit is off while conflicted', await panel.getByRole('button', { name: /Commit merge/ }).isDisabled());
  await section('Conflicts').getByRole('button', { name: USER }).click();
  const conflict = panel.getByRole('region', { name: `Conflict in ${USER}` });
  const ctext = await conflict.innerText().catch(() => '');
  check('conflict shows ours, theirs and base', ctext.includes('users/1/mine') && ctext.includes('/v2/users/1') && ctext.includes('Base'), ctext.slice(0, 300));
  await conflict.getByRole('button', { name: 'Take theirs' }).click();
  await settle(p);
  check('taking theirs resolves it', (await panel.locator('.git-merging').innerText()).includes('all conflicts are resolved'));
  await panel.getByRole('button', { name: /Commit merge/ }).click();
  await settle(p);
  const merge1 = git(r.work, 'log', '-1', '--format=%p|%s');
  check('merge commit has two parents', merge1.split('|')[0].split(' ').length === 2 && merge1.includes('Merge'), merge1);
  check('file is theirs', fs.readFileSync(path.join(r.work, USER), 'utf8').includes('url: ${base}/v2/users/1'));
  check('state clean after the merge', !(await panel.locator('.git-merging').count()) && (await bar()).includes('↑2 ↓0'), await bar());
  await panel.getByRole('button', { name: /Push/ }).click();
  await lastNote(/Pushed main.*\n?/);
  await settle(p);

  // Conflict 2: resolved with an edited text.
  otherPushesUrl(r, '${base}/v3/users/1');
  await editUrl('${base}/v2/users/1/again?fields=name,email');
  await gitPage();
  await commitAll('Again');
  notes.length = 0;
  await panel.getByRole('button', { name: /Pull/ }).click();
  await lastNote(/Pull stopped/);
  await settle(p);
  await section('Conflicts').getByRole('button', { name: USER }).click();
  await settle(p);
  const resolution = panel.locator('#git-resolution');
  const merged = await resolution.evaluate((el) => el.value);
  check('the editor starts from the file with markers', merged.includes('<<<<<<<') && merged.includes('>>>>>>>'), merged.slice(0, 200));
  await p.getByRole('button', { name: 'Use this text and mark resolved' }).click();
  check('markers left in are refused', /conflict markers/.test(await lastNote(/conflict markers/)));
  const edited = merged.replace(/<<<<<<< [^\n]*\n[\s\S]*?>>>>>>> [^\n]*\n/, 'url: ${base}/v3/users/1/again\n');
  await resolution.evaluate((el, t) => (el.value = t), edited);
  await p.getByRole('button', { name: 'Use this text and mark resolved' }).click();
  await settle(p);
  await panel.getByRole('button', { name: /Commit merge/ }).click();
  await settle(p);
  check('edited resolution committed', fs.readFileSync(path.join(r.work, USER), 'utf8').includes('url: ${base}/v3/users/1/again') && git(r.work, 'log', '-1', '--format=%p').split(' ').length === 2);
  check('the app shows the resolved request', (await p.locator('aside').innerText()).includes('Get user'));
  await panel.getByRole('button', { name: /Push/ }).click();
  await lastNote(/Pushed/);
  git(r.other, 'pull', '-q', '--no-rebase');
  check('the teammate gets the merge', fs.readFileSync(path.join(r.other, USER), 'utf8').includes('/v3/users/1/again'));

  // Delete the merged branch.
  await settle(p);
  await btn('Delete feature').click();
  await confirmDialog('Delete');
  check('merged branch deleted', !git(r.work, 'branch', '--list', 'feature'));

  // An HTTPS remote: a token for its host goes to the keychain only.
  git(r.work, 'remote', 'set-url', 'origin', 'https://git.example.test/team/shop.git');
  await refresh();
  const access = section('Remote access');
  check('token form for the HTTPS host', (await access.innerText()).includes('git.example.test'), await access.innerText().catch(() => ''));
  await access.locator('#git-token').evaluate((el) => (el.value = 'tok-e2e-123'));
  await access.getByRole('button', { name: 'Save token' }).click();
  await settle(p);
  check('token in the keychain as git/<host>/token', h.mock.keychain.get('git/git.example.test/token') === 'tok-e2e-123');
  check('token in no file', !JSON.stringify(listTree(r.work)).includes('tok-e2e-123') && !fs.readFileSync(path.join(r.work, '.git/config'), 'utf8').includes('tok-e2e-123') && !JSON.stringify(listTree(h.mock.config)).includes('tok-e2e-123'));
  check('saved shown', (await access.innerText()).includes('saved in your system keychain'));
  await access.getByRole('button', { name: 'Remove token' }).click();
  await settle(p);
  check('token removed', !h.mock.keychain.has('git/git.example.test/token'));
  git(r.work, 'remote', 'set-url', 'origin', r.remote);

  check('the panel used hitop-git (libgit2)', ['status', 'diff', 'stage', 'unstage', 'commit', 'push', 'createBranch', 'switchBranch', 'fetch', 'pull', 'conflict', 'resolve', 'deleteBranch'].every((c) => h.mock.gitCalls.includes(c)));
  check('no page errors', errors.length === 0, errors.join(' || '));
  fs.rmSync(r.root, { recursive: true, force: true });
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message.split('\n')[0]);
  process.exit(1);
});
