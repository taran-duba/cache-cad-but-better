# Testing cadsync on Windows

cadsync has only been tested on macOS against a fake Google Drive. This guide checks that it
really works for the team: real Drive, real SolidWorks, two people at once. Work through it in
order, and fill in the results table at the end.

> **Use a test shared drive, not the team's real one.** Some steps break locks, kill the app
> mid-upload, and create junk files.

## What you need

- **Two Windows PCs** with SolidWorks installed, called **PC A** and **PC B** below. A Mac can
  stand in for PC B in parts 3 and 4, but the SolidWorks steps (parts 5–7) need two Windows PCs.
- **Two Google accounts**, **Alex** on PC A and **Blair** on PC B. Both need to be members of the
  test shared drive with **Content manager** access.
- The team's **OAuth client id and secret** and **Document Manager key** (see
  [SETUP.md](SETUP.md), steps 1 and 3).
- A small test assembly with **mates**: an assembly plus 2–3 parts, all in one folder.
  Something like a plate, a shaft and a gear with concentric and coincident mates is fine.

## Part 1: Build and run the automated tests (PC A)

1. Install the prerequisites from the README section "Build the Windows installer": Node.js +
   pnpm, Rust, the C++ Build Tools, WebView2, and the .NET 8 SDK.
2. In PowerShell at the repo root:

   ```powershell
   pnpm install
   pnpm sidecar
   cargo test --manifest-path src-tauri/Cargo.toml
   dotnet test sidecar/SwRefs.Tests
   ```

   `pnpm sidecar` builds the SwRefs helper with the real SolidWorks reader, and fails if it
   can't find SolidWorks' interop DLL. Set `SWDM_INTEROP_PATH` as described in the README if
   SolidWorks isn't installed in the default location.

**Expected:** every test passes. These tests have never run on Windows, so a failure that only
happens on Windows is an important result. Copy the full output into your report.

## Part 2: Reference helper against a real assembly (PC A)

This checks that the Document Manager key works and that SwRefs reads real SolidWorks files.

1. Copy the test assembly and its parts to a folder, e.g. `C:\SwRefsTest\`.
2. Run:

   ```powershell
   $env:CADSYNC_SWDM_KEY = "<the Document Manager key>"
   '{"id":"1","op":"ping"}' | .\src-tauri\binaries\SwRefs-x86_64-pc-windows-msvc.exe
   '{"id":"2","op":"refs","path":"C:\\SwRefsTest\\TestAssembly.SLDASM"}' | .\src-tauri\binaries\SwRefs-x86_64-pc-windows-msvc.exe
   ```

**Expected:**
- [ ] `ping` returns `"ok":true` with a version.
- [ ] `refs` returns `"ok":true` with one entry per part. Each `path` is a full path to a part,
  and `exists` is `true`.
- [ ] The list matches what SolidWorks shows for the assembly under **File › Find References**.
- [ ] With a wrong key, `refs` returns `"code":"NO_LICENSE"`.

## Part 3: Set up both PCs

On **both** PCs:

1. Run the app with logging, from the repo root:

   ```powershell
   $env:RUST_LOG = "cadsync=debug"
   pnpm tauri dev
   ```

   Running from source shows logs in the terminal. The installed app has no console window. To
   test the installer instead, build it with `pnpm tauri build` and install the `.exe` from
   `src-tauri\target\release\bundle\nsis\`.
2. In **Settings**, enter:
   - the OAuth client id and secret
   - the **test** shared drive's URL
   - the vault folder: `C:\CAD\Test Drive`, **the same on both PCs**
   - the Document Manager key
3. Click **Save and sign in with Google**, sign in as Alex (PC A) or Blair (PC B), and accept the
   "unverified app" warning.
4. In **Vault**, tick **Get** on the top-level folder.

**Expected:**
- [ ] The browser opens Google sign-in. Afterwards the app shows your email in the top bar.
- [ ] The sidebar does **not** show "Reference checking is off". If it does, the key or SwRefs
  isn't set up.
- [ ] After a sync (click **Sync now**, or wait 15 s), files appear under `C:\CAD\Test Drive`.
- [ ] Files you haven't checked out are **read-only**: right-click › Properties shows
  *Read-only* ticked.

## Part 4: Drive behaviour (the riskiest assumptions)

The design relies on these Google Drive behaviours, which have only been read about in the
docs, never tested. If any of them fails, stop and report it before going further.

### 4a. A check-out blocks uploads from the Drive website

1. **PC A:** add the test assembly through cadsync, so its references point inside the vault:
   1. Copy the assembly and its parts into `C:\CAD\Test Drive\Test Assembly\`.
   2. Open the assembly **from that folder** in SolidWorks, do **Save All**, and close it.
   3. In cadsync, go to **My changes**, tick all the new files, click **Review check-in**, and
      check them in.

   **Expected:**
   - [ ] No problems are listed, and the check-in succeeds.
   - [ ] After syncing, **PC B** has the files under the same path.
2. **PC A:** in Vault, click **Check out** on one part.
3. **PC B:** in a browser signed in as Blair, open the test shared drive. Right-click that part,
   choose **File information › Manage versions › Upload new version**, and pick any file.

**Expected:**
- [ ] PC B's app shows the part as checked out by Alex (🔒 alex) within about 15 s.
- [ ] **Drive refuses Blair's upload** (the file is locked or read-only). If the upload
  succeeds, write that down. cadsync users are still protected, but the Drive website isn't.

### 4b. Only one person can check out

1. **PC B:** click **Check out** on the same part.

**Expected:**
- [ ] An error says the part is checked out by alex.

### 4c. Versions are kept forever

1. **PC A:** check in the part (see 5a for how). Then in the browser, open the part's
   **Manage versions**.

**Expected:**
- [ ] The new version is marked **Keep forever**.

## Part 5: The broken-mates fix

### 5a. An assembly and its parts arrive together

1. **PC A:** check out the assembly and one of its parts.
2. In SolidWorks on PC A, open the assembly. Change a dimension in the part that moves a mated
   face, e.g. make the plate thicker or move a hole. Rebuild, and save both files.
3. In cadsync, go to **My changes**, tick **only the assembly**, then click **Review check-in**.

   **Expected:**
   - [ ] The part you edited appears under "Added automatically because your assemblies use them".
   - [ ] No problems are listed.
4. Type a message and click **Check in**.
5. **PC B:** wait for the sync (or click **Sync now**), then open the assembly in SolidWorks.

**Expected:**
- [ ] PC B shows the new part geometry.
- [ ] **No mate errors or warnings** in the FeatureManager tree, and no missing-reference prompt.
- [ ] **History** on PC B lists the check-in with Alex's message and 2 files.

### 5b. Checks before every check-in

Run each case on PC A, click **Review check-in**, note what it shows, and then undo the change.

| Case | How to set it up | Expected problem |
| --- | --- | --- |
| File outside the vault | In the checked-out assembly, insert a part saved on the **Desktop** | "File outside the vault" |
| Duplicate name | Copy a part into a second folder in the vault (so two folders have `Spacer.SLDPRT`), check that copy in, then check in an assembly that uses one of them | "Same file name in several folders". Ticking "Check in anyway…" allows it. |
| Missing file | Make the assembly reference a part, close SolidWorks, then delete or rename that part in Explorer | "Missing file" |
| Edited without checking out | Clear Read-only on a part the assembly uses, edit it in SolidWorks, then check in the assembly | "Edited without checking out" |
| New part | Create a new part in the vault and add it to the checked-out assembly | No problem. The new part is added automatically and shows **new**. |

- [ ] Each case behaves as expected. The **Check in** button stays disabled while there are
  problems, apart from the duplicate-name override.

## Part 6: Safety nets

### 6a. Read-only stops accidental overwrites

1. **PC B:** open a part in SolidWorks that Blair has **not** checked out. Edit it and press Save.

**Expected:**
- [ ] SolidWorks shows the file as read-only and won't save over it (it offers Save As).

### 6b. Updates wait while a file is open

1. **PC B:** open the assembly in SolidWorks and keep it open.
2. **PC A:** check out, edit, and check in a part of that assembly.
3. **PC B:** watch the top bar for about 30 s, then close the assembly in SolidWorks.

**Expected:**
- [ ] While it's open, the top bar says "… waiting for SolidWorks to close", and the files don't
  change.
- [ ] Within about 15 s of closing, the update arrives.
- [ ] Look in the part's folder for a `~$PartName.SLDPRT` file while it's open. cadsync uses
  that file to tell that SolidWorks has a document open. **Write down whether it appears.** If
  it doesn't, this check can't work.

### 6c. A crash during check-in

1. **PC A:** to make the upload slow, create a big file in the vault:

   ```powershell
   fsutil file createnew "C:\CAD\Test Drive\big-test-file.bin" 500000000
   ```

2. Check out a part and edit it. In **My changes**, tick the part **and** `big-test-file.bin`,
   then click **Check in**.
3. While it says "Checking in…", end **cadsync** in Task Manager.
4. **PC B:** click **Sync now**.
5. **PC A:** start cadsync again and wait about 30 s.

**Expected:**
- [ ] After step 4, PC B gets **neither** file, even if part of the upload finished. Anything
  already uploaded shows as "Being checked in".
- [ ] After step 5, PC A finishes the check-in on its own within about 30 s. The top bar may
  briefly show "1 check-in(s) waiting to finish". The big file starts uploading again from the
  beginning.
- [ ] PC B then gets the part and the big file together.
- [ ] Delete `big-test-file.bin` from the test drive afterwards.

### 6d. Edits made without checking out are kept

1. **PC B:** clear Read-only on a part (Properties), edit it in SolidWorks, and save.
2. **PC A:** check out the same part, change it, and check it in.
3. **PC B:** sync.

**Expected:**
- [ ] PC B now has Alex's version. Blair's edits are kept next to it as
  `Part (conflict blair <date>).SLDPRT`, and a message says so.

### 6e. Breaking a lock

1. **PC A:** check out a part and edit it, but don't check it in.
2. **PC B:** click **Break lock** on it, confirm, then check it out yourself.
3. **PC A:** sync, then look at **My changes**.

**Expected:**
- [ ] PC A shows the part as checked out by blair.
- [ ] The part is gone from Alex's **My changes**, so it can't be checked in over Blair's lock.
- [ ] Alex's edited file on PC A is not touched.

### 6f. Names Windows doesn't allow

In the Drive website, create:
- a folder named `Intake/Shooter`
- a file named `CON.txt`
- a file whose name starts with an emoji, e.g. `🤖 Notes.txt`
- two files named `Test.txt` and `test.txt` in the same folder

Then sync.

**Expected:**
- [ ] The app doesn't crash. Locally they appear as `Intake_Shooter`, `_CON.txt` and
  `🤖 Notes.txt`. Of the two `test` files, the one created first keeps its name and the other
  gets ` (2)`.
- [ ] In Vault, renamed files show "→ local name" next to the Drive name.

## Part 7: Speed (on the team's real drive)

This part is read-only (only tick **Get** on folders), so it's safe to run on the real team drive.

1. Point one PC at the real drive (Settings › Shared drive), sign in, and time how long until the
   Vault tab lists the top-level folders.
2. Quit and restart the app. Time how long until the folder list appears.
3. Have someone change a file on Drive, then click **Sync now**. Time how long until it finishes.

**Targets:**
- [ ] Restart: under 1 second.
- [ ] Sync with nothing to download: under 2 seconds.
- [ ] First load: no target, just write the time and roughly how many files the drive has.

## Where things are

| What | Where |
| --- | --- |
| Settings | `%APPDATA%\org.frc.cadsync\config.json` |
| Local index (safe to delete; it rebuilds) | `%APPDATA%\org.frc.cadsync\<drive id>\index.sqlite` |
| Files removed from the vault | `%APPDATA%\org.frc.cadsync\<drive id>\trash\` |
| Check-in records on Drive | `.cadsync/changesets/` in the shared drive |
| Logs | The terminal running `pnpm tauri dev` (with `RUST_LOG=cadsync=debug`) |

## Results

Copy this table into your report. For every **fail**, include what you did, what happened, and
the log lines from around that time.

| Test | Pass / Fail | Notes |
| --- | --- | --- |
| 1. Automated tests | | |
| 2. SwRefs on a real assembly | | |
| 3. Setup on both PCs | | |
| 4a. Check-out blocks Drive website upload | | |
| 4b. Only one check-out | | |
| 4c. Keep forever | | |
| 5a. Assembly + parts arrive together, mates intact | | |
| 5b. Pre-check-in checks (5 cases) | | |
| 6a. Read-only stops saves | | |
| 6b. Updates wait for SolidWorks (did `~$` appear?) | | |
| 6c. Crash during check-in | | |
| 6d. Conflict copy kept | | |
| 6e. Break lock | | |
| 6f. Odd names | | |
| 7. Speed (times and file count) | | |
