Free desktop build of HITOP for macOS, Windows and Linux. These builds aren't signed with a paid
certificate, so your system warns the first time you open them. The
[Mac App Store version](https://apps.apple.com/app/hitop-api-client/id6785294166) is signed and
updates itself.

**macOS** (`.dmg`, Apple silicon and Intel): drag HITOP to Applications. If macOS says the app
"is damaged" or "can't be opened", run this once in Terminal:

```bash
xattr -dr com.apple.quarantine /Applications/HITOP.app
```

**Windows** (`.msi` or `-setup.exe`): if SmartScreen shows "Windows protected your PC", click
**More info**, then **Run anyway**.

**Linux**: `.AppImage` (`chmod +x` then run it), `.deb` (`sudo apt install ./HITOP_*.deb`) or
`.rpm`.

Checksums are on each asset's page. Report problems at
https://github.com/bohdaq/hitop/issues.
