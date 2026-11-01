// /download/: replace the releases-page links with direct links to the newest stable release of
// each kind (desktop-v*, cli-v*). Without JS, or if GitHub doesn't answer, the links still work.
export async function initDownloads() {
  const links = document.querySelectorAll("[data-release][data-asset]");
  if (!links.length) return;
  let releases;
  try {
    const res = await fetch("https://api.github.com/repos/bohdaq/hitop/releases?per_page=50");
    if (!res.ok) return;
    releases = await res.json();
  } catch {
    return;
  }
  const latest = (kind) => releases.find((r) => !r.draft && !r.prerelease && r.tag_name.startsWith(`${kind}-v`));
  for (const kind of ["desktop", "cli"]) {
    const release = latest(kind);
    if (!release) continue;
    const note = document.querySelector(`[data-release-note="${kind}"]`);
    if (note) note.append(` Latest: ${release.tag_name.replace(`${kind}-v`, "")}.`);
    for (const a of document.querySelectorAll(`[data-release="${kind}"]`)) {
      const re = new RegExp(a.dataset.asset);
      const asset = release.assets.find((x) => re.test(x.name));
      if (asset) a.href = asset.browser_download_url;
    }
  }
}
