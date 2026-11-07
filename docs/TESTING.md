# Testing Permafrost on Windows

CI builds and installs Permafrost on a Windows runner on every push to `main`, and a teammate has
installed a release on a real Windows PC with SolidWorks. The sync logic itself has mostly been
tested against a fake Google Drive. This guide checks that it really works for the team: real
Drive, real SolidWorks, two people at once. Work through it in order, and fill in the results
table at the end.

> **Use a test shared drive, not the team's real one.** Some steps break locks, kill the app
> mid-upload, rename files for everyone, and create junk files.

## What you need

- **Two Windows PCs** with SolidWorks installed, called **PC A** and **PC B** below. A Mac can
  stand in for PC B in parts 3 and 4, but the SolidWorks steps (parts 5 and 6) need two Windows
  PCs.
- **Two Google accounts**, **Alex** on PC A and **Blair** on PC B. Both need to be members of the
  test shared drive with **Content manager** access. Name the drive **Test Drive** so the paths
  in this guide (`C:\CAD\Test Drive`) match.
- Only if you build from source: the Google client file saved as `src-tauri/google-oauth.json`
  before building. Release builds have Google sign-in built in.
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

   `pnpm sidecar` builds the SwRefs helper that the app and the tests below use.

**Expected:** every test passes. CI runs these on a Windows runner without SolidWorks, so a
failure that only happens on a PC with SolidWorks installed is an important result. Copy the full
output into your report.

## Part 2: Reference helper against a real assembly (PC A)

This checks that SwRefs reads real SolidWorks files through SolidWorks. It has run against real
SolidWorks once: it hit COM error `0x8002802B`, which now has a fallback. Write down what happens,
including any error codes.

Use the SwRefs you built in Part 1 (`.\src-tauri\binaries\SwRefs-x86_64-pc-windows-msvc.exe`) or
the one installed next to `permafrost.exe` (`SwRefs.exe`, usually in `%LOCALAPPDATA%\permafrost\`).

1. Copy the test assembly and its parts to a folder, e.g. `C:\SwRefsTest\`.
2. Run it once with SolidWorks already open, then once with SolidWorks closed:

   ```powershell
   '{"id":"1","op":"ping"}' | .\src-tauri\binaries\SwRefs-x86_64-pc-windows-msvc.exe
   '{"id":"2","op":"refs","path":"C:\\SwRefsTest\\TestAssembly.SLDASM"}' | .\src-tauri\binaries\SwRefs-x86_64-pc-windows-msvc.exe
   ```

3. Copy one part to a new name (e.g. `Plate.SLDPRT` → `Plate2.SLDPRT`), then repoint the assembly
   at the copy. Use the exact `path` that `refs` returned for `old`:

   ```powershell
   '{"id":"3","op":"replace","path":"C:\\SwRefsTest\\TestAssembly.SLDASM","old":"C:\\SwRefsTest\\Plate.SLDPRT","new":"C:\\SwRefsTest\\Plate2.SLDPRT"}' | .\src-tauri\binaries\SwRefs-x86_64-pc-windows-msvc.exe
   ```

**Expected:**
- [ ] `ping` returns `"ok":true` with a version, even with SolidWorks closed.
- [ ] SolidWorks open: `refs` answers `"ok":true` in a few seconds, one entry per part. Each `path`
  is a full path to a part, `exists` is `true`, and the list matches **File › Find References**.
  SolidWorks stays open.
- [ ] SolidWorks closed: the first `refs` takes up to a minute, answers the same list, and no
  `sldworks.exe` is left in Task Manager afterwards.
- [ ] Add a part that doesn't exist on disk to the assembly's references (delete it after saving):
  it comes back with `"exists":false` and the path stored in the assembly, not a path SolidWorks
  searched for. **If it comes back with a different path, tell me: the `GetDocumentDependencies2`
  search flag needs flipping.**
- [ ] A virtual component (if the assembly has one) comes back with `"isVirtual":true`.
- [ ] `replace` answers `"ok":true`. Running `refs` again lists `Plate2.SLDPRT` instead of
  `Plate.SLDPRT`, and opening the assembly in SolidWorks shows no mate errors.

## Part 3: Set up both PCs

On **both** PCs:

1. Install the latest release: download the Windows installer (`.exe`) from
   <https://github.com/taran-duba/permafrost/releases> and run it. CI installs this same installer
   on every push to `main` and checks that `SwRefs.exe` lands next to `permafrost.exe`.

   To see logs in a terminal instead, run from source at the repo root:

   ```powershell
   $env:RUST_LOG = "cadsync=debug"
   pnpm tauri dev
   ```

   The installed app has no console window; its logs are in Settings › **Open logs folder**.
2. Click **Sign in with Google**, sign in as Alex (PC A) or Blair (PC B), accept the
   "unverified app" warning, and allow Drive access.
3. Pick the **test** folder: browse to it (or paste a link) and click **Use this folder**.
4. Wait for the first sync. Everything downloads; there's nothing to tick.

**Expected:**
- [ ] The browser opens Google sign-in. Afterwards the app shows your shared drives, My Drive and
  Shared with me. Browsing into a folder shows its subfolders, and "Use" opens it. The top bar
  then shows the folder name and your email.
- [ ] The vault folder is `C:\CAD\<folder name>` on both PCs (Settings › Advanced).
- [ ] The sidebar does **not** show "Reference checking is off". If it does, SwRefs isn't next
  to the app.
- [ ] After a sync (click **Sync now**, or wait 15 s), files appear under `C:\CAD\Test Drive`.
- [ ] Files you haven't checked out are **read-only**: right-click › Properties shows
  *Read-only* ticked.
- [ ] Every file in the test folder, in every subfolder, is on both PCs.

## Part 4: Drive behaviour (the riskiest assumptions)

The design relies on these Google Drive behaviours, which have only been read about in the
docs, never tested. If any of them fails, stop and report it before going further.

### 4a. A check-out blocks uploads from the Drive website

1. **PC A:** add the test assembly through Permafrost, so its references point inside the vault:
   1. Copy the assembly and its parts into `C:\CAD\Test Drive\Test Assembly\`.
   2. Open the assembly **from that folder** in SolidWorks, do **Save All**, and close it.
   3. In Permafrost, go to **My changes**, tick all the new files, click **Review changes to
      push**, type a message, and click **Push changes**.

   **Expected:**
   - [ ] No problems are listed, and the push succeeds.
   - [ ] After syncing, **PC B** has the files under the same path.
2. **PC A:** in Vault, click **Check out** on one part.
3. **PC B:** in a browser signed in as Blair, open the test shared drive. Right-click that part,
   choose **File information › Manage versions › Upload new version**, and pick any file.

**Expected:**
- [ ] PC B's app shows the part as checked out by Alex (🔒 alex) within about 15 s.
- [ ] **Drive refuses Blair's upload** (the file is locked or read-only). If the upload
  succeeds, write that down. Permafrost users are still protected, but the Drive website isn't.

### 4b. Only one person can check out

1. **PC B:** click **Check out** on the same part.

**Expected:**
- [ ] An error says the part is checked out by alex.

### 4c. Versions are kept forever

1. **PC A:** push the part (see 5a for how). Then in the browser, open the part's
   **Manage versions**.

**Expected:**
- [ ] The new version is marked **Keep forever**.

## Part 5: The broken-mates fix

### 5a. An assembly and its parts arrive together

1. **PC A:** click **Check out** on the assembly. Its parts are checked out with it.
2. In SolidWorks on PC A, open the assembly (the row's **Open** button works). Change a dimension
   in one part that moves a mated face, e.g. make the plate thicker or move a hole. Rebuild, and
   save both files.
3. In Permafrost, go to **My changes**, tick **only the assembly**, then click **Review changes
   to push**.

   **Expected:**
   - [ ] The part you edited appears under "Added automatically because your assemblies use them".
   - [ ] No problems are listed.
4. Type a message, leave **Also release the N checked-out file(s) you didn't edit** ticked, and
   click **Push changes**.
5. **PC B:** wait for the sync (or click **Sync now**), then open the assembly in SolidWorks.

**Expected:**
- [ ] PC B shows the new part geometry.
- [ ] **No mate errors or warnings** in the FeatureManager tree, and no missing-reference prompt.
- [ ] **History** on PC B lists the push with Alex's message and 2 files.
- [ ] On PC A the parts you didn't edit are released too (nothing left in **My changes**), and the
  toast says how many were released.

### 5b. Checks before every push

Run each case on PC A with the assembly checked out, click **Review changes to push**, note what
it shows, and then undo the change.

| Case | How to set it up | Expected |
| --- | --- | --- |
| File outside the vault | In the assembly, insert a part saved on the **Desktop** whose name isn't used by any file in the vault | "File outside the vault" |
| Outside the vault, two matches | Same, but name the Desktop part like a file that exists in **two** vault folders | "File outside the vault", suggesting one of the vault copies |
| Old path, one match | Save a copy of a vault part (one whose name exists only once in the vault) to the Desktop and replace the component with that copy | No problem. A quiet note says the reference still has an old path from outside the vault but matches one vault file, and to clean it up by opening the assembly, Ctrl+Q and save. |
| Duplicate name | Push a copy of `Spacer.SLDPRT` into a second folder (so two folders have it). Make a new local folder `C:\CAD\Test Drive\Scratch\`, copy Spacer there (don't push it), replace the assembly's Spacer with that copy, save, close SolidWorks, then delete the Scratch folder in Explorer | "Same file name in several folders" **and** "Missing file". A reference whose exact path is in the vault does **not** trigger this, even with same-named files elsewhere. |
| Missing file | Make the assembly reference a part, close SolidWorks, then delete or rename that part in Explorer | "Missing file" |
| Edited without checking out | **Undo check-out** on one of the assembly's parts, clear Read-only on it, edit it in SolidWorks, then push the assembly | "Edited without checking out" |
| New part | Create a new part in the vault and add it to the checked-out assembly | No problem. The new part is added automatically and shows **new**. |
| SolidWorks can't read it | In **My changes**, review a junk file named `Broken.SLDASM` (a renamed text file), or lock the checked-out assembly with `$f = [IO.File]::Open("<path>", 'Open', 'Read', 'None')` in PowerShell (`$f.Close()` afterwards) | Not blocked. A yellow notice names the file and the error. For the locked assembly it says the parts it used last time were checked instead, then "If you changed any of its parts, tick them above before pushing". |

- [ ] Each case behaves as expected. The **Push changes** button stays disabled while there are
  problems. The duplicate-name case is listed only as "Same file name in several folders" (not
  also as a missing file), and ticking "Push anyway with duplicate names…" enables the button.

### 5c. Checking out takes an assembly's parts along

1. Put one part the assembly uses in a different folder (rename/move it as in 6j, or build a
   second assembly that uses a part from another folder).
2. **PC B:** check out one of the assembly's parts.
3. **PC A:** click **+ parts** on the assembly.
4. **PC A:** undo all check-outs, then open the test folder and click **Check out all**.
5. **PC A:** click **Open** on a part.

**Expected:**
- [ ] After step 3, the assembly and every part it uses (including sub-assemblies' parts and the
  one in the other folder) are checked out by Alex. The toast names the part Blair holds and says
  it was skipped.
- [ ] After step 4, every file in the folder and its subfolders that nobody else holds is checked
  out, plus parts its assemblies use from other folders.
- [ ] After step 5, the part opens in SolidWorks.
- [ ] Undo every check-out on both PCs afterwards.

## Part 6: Safety nets

### 6a. Read-only stops accidental overwrites

1. **PC B:** open a part in SolidWorks that Blair has **not** checked out. Edit it and press Save.

**Expected:**
- [ ] SolidWorks shows the file as read-only and won't save over it (it offers Save As).

### 6b. Updates wait while a file is open

1. **PC B:** open the assembly in SolidWorks and keep it open.
2. **PC A:** check out, edit, and push a part of that assembly.
3. **PC B:** watch the top bar for about 30 s, then close the assembly in SolidWorks.

**Expected:**
- [ ] While it's open, the top bar says "… waiting for SolidWorks to close", and the files don't
  change.
- [ ] Within about 15 s of closing, the update arrives.
- [ ] Look in the part's folder for a `~$PartName.SLDPRT` file while it's open. Permafrost uses
  that file to tell that SolidWorks has a document open. **Write down whether it appears.** If
  it doesn't, this check can't work.

### 6c. A crash during a push

1. **PC A:** to make the upload slow, create a big file in the vault:

   ```powershell
   fsutil file createnew "C:\CAD\Test Drive\big-test-file.bin" 500000000
   ```

2. Check out a part and edit it. In **My changes**, tick the part **and** `big-test-file.bin`,
   click **Review changes to push**, type a message, then click **Push changes**.
3. While it says "Pushing…", end **Permafrost** in Task Manager.
4. **PC B:** click **Sync now**.
5. **PC A:** start Permafrost again and wait about 30 s.

**Expected:**
- [ ] After step 4, PC B gets **neither** file, even if part of the upload finished. Anything
  already uploaded shows as "Being pushed".
- [ ] After step 5, PC A finishes the push on its own within about 30 s. The top bar may
  briefly show "1 push(es) waiting to finish". The big file starts uploading again from the
  beginning.
- [ ] PC B then gets the part and the big file together.
- [ ] Delete `big-test-file.bin` from the test drive afterwards.

### 6d. Edits made without checking out are kept

1. **PC B:** clear Read-only on a part (Properties), edit it in SolidWorks, and save.
2. **PC A:** check out the same part, change it, and push it.
3. **PC B:** sync.

**Expected:**
- [ ] PC B now has Alex's version. Blair's edits are kept next to it as
  `Part (conflict blair <date>).SLDPRT`, and a message says so.

### 6e. Taking over a lock

1. **PC A:** check out a part and edit it, but don't push it.
2. **PC B:** click **Need it?** on it (or the 🔒 pill), then **Take it over…** and **Yes, take it
   over**. This breaks Alex's lock and checks the part out to Blair.
3. **PC A:** sync, then look at **My changes**.

**Expected:**
- [ ] PC A shows the part as checked out by blair.
- [ ] The part is gone from Alex's **My changes**, so it can't be pushed over Blair's lock.
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

### 6g. Deleted files come back; Delete removes for everyone

1. **PC B:** in Explorer, delete a part from the vault folder (clear Read-only first if Windows
   asks). Click **Sync now**.
2. **PC A:** in Vault, click **Delete** on a different part that no assembly uses, and confirm.
3. **PC A:** click **Delete** on a part the test assembly uses.

**Expected:**
- [ ] After step 1 the part is back on PC B, with a message that it was put back.
- [ ] After step 2 the part disappears on both PCs. It's in the shared drive's **Trash** on the
  Drive website, and PC A's copy is in the app's trash folder.
- [ ] In step 3, Permafrost warns "… is used by TestAssembly.SLDASM" before deleting. This only works
  if that assembly's references were read when it was pushed.

### 6g2. A folder inside a bigger drive

1. In the test shared drive, make a folder **Test Drive** (or use the one you have) that sits
   next to other folders and files. Pick **only that folder** in step 3 above on both PCs.
2. Put a file next to it, outside the folder, and another inside.

**Expected:**
- [ ] Only files inside the picked folder appear in Vault and under `C:\CAD\Test Drive`.
- [ ] After a push, a hidden `.permafrost` folder appears **inside** the picked folder in Drive,
  not at the top of the drive. (If the folder already has a `.cadsync` folder from an older version, that one is
  reused and no new folder is created.)
- [ ] Dragging a subfolder with files into the picked folder (in the Drive website) makes it
  appear on both PCs within about 15 s. Dragging one out makes it disappear.
- [ ] A link to a folder pasted into the picker opens it too.

### 6g3. A folder shared with you from a drive you're not in

1. With a third Google account (Casey) that is **not** a member of the shared drive, share just
   the test folder with Casey as **Editor** (Drive website › Share).
2. On a PC, sign in as Casey and pick the folder from **Shared with me**.

**Expected:**
- [ ] The folder opens and its files sync. No "not found" or "forbidden" error.
- [ ] Check out and push work for Casey.
- [ ] Afterwards, add Casey to the shared drive as a member and wait about 30 s. Sync keeps
  working with no restart.

### 6h. Only one copy runs

1. With Permafrost open, start it again from the Start menu.

**Expected:**
- [ ] No second window opens. The existing window comes to the front.

### 6i. Expired sign-in

1. Go to <https://myaccount.google.com/connections>, find the Permafrost app, and remove its access.
2. Wait about 30 s, or click **Sync now**.

**Expected:**
- [ ] Permafrost goes back to the welcome screen with "Your Google sign-in expired or was removed".
- [ ] After signing in again, the same drive opens, and files and check-outs are as before.

### 6j. Renaming and moving

Start with nothing checked out on either PC and the test assembly closed in SolidWorks on both.

1. **PC A:** in Vault, click **Rename** on a part the test assembly uses, e.g. `Plate.SLDPRT`.
   The box shows `Test Assembly/Plate.SLDPRT`. Change it to `Test Assembly/Base Plate.SLDPRT`
   and click **Rename / move**.
2. **PC B:** sync, then open the assembly in SolidWorks.
3. **PC A:** click **Rename** on the `Test Assembly` folder and change it to
   `Robot/Test Assembly`. Then repeat step 2.
4. **PC B:** check out the assembly. **PC A:** try to rename one of its parts. Then have PC B
   undo the check-out.
5. **PC A:** try these, one at a time: open the assembly in SolidWorks and rename a part; rename a
   part to `Ba?se.SLDPRT`; rename `.SLDPRT` to `.SLDASM`; rename to a name that already exists;
   move the folder into its own subfolder.
6. **PC A:** in Explorer, rename a part the assembly uses (clear Read-only first), then sync.

**Expected:**
- [ ] After step 1, the toast says the part is now `Test Assembly/Base Plate.SLDPRT` and that it
  updated and pushed 1 assembly/drawing. **History** shows one push, "Renamed Test
  Assembly/Plate.SLDPRT to Test Assembly/Base Plate.SLDPRT", with the part and the assembly.
  Nothing is left checked out.
- [ ] After step 2, PC B has `Base Plate.SLDPRT` and no `Plate.SLDPRT`. The assembly opens with
  **no mate errors** and no missing-reference prompt, and **File › Find References** shows the new
  name. PC B never had the new name without the updated assembly (watch the folder during sync).
- [ ] After step 3, History shows "Moved Test Assembly to Robot/Test Assembly", and PC B gets the
  files in the new folder with the assembly opening cleanly.
- [ ] In step 4, PC A gets an error that the assembly is checked out by blair. Nothing changes on
  either PC or on Drive.
- [ ] In step 5, each attempt is refused with a reason (open in SolidWorks, name not allowed on
  Windows, keep the same file type, already there, can't move inside itself). Nothing changes.
- [ ] In step 6, the Explorer rename is **not** treated as a rename. Write down what Permafrost
  does (expect the old file put back and the new name shown as a new file). Use **Rename** in
  Permafrost instead; undo the Explorer rename afterwards.
- [ ] With reference checking off (or on the Mac), renaming a `.SLDPRT` is refused with a message
  that it needs SolidWorks reference checking. Renaming a `.txt` file still works.
- [ ] If SolidWorks fails part-way (e.g. it crashes) before anything reached Drive, the part has
  its old name again, the assembly is unchanged, and nothing is left checked out. Note it if you
  see this.

### 6k. Restoring old versions

1. **PC A:** check out the assembly, change the part so it looks clearly different ("v2"), and push
   both with the message `v2`. Do it again with the message `v3`.
2. **PC A:** in **History**, click the `v2` push to see its files. Click **Restore all files from
   this push** and confirm ("Put … back to this version for everyone? Newer versions stay in
   History.").
3. **PC B:** sync, then open the assembly in SolidWorks.
4. **PC A:** restore the `v2` push again.
5. **PC B:** check out the part. **PC A:** click **Restore this file** on the part in the `v3`
   push. Then have PC B undo the check-out.

**Expected:**
- [ ] After step 2, History shows a new push `Restored "v2" (<date>)` at the top, and the `v2` and
  `v3` pushes are still listed.
- [ ] After step 3, PC B has the `v2` assembly **and** part, with **no mate errors**.
- [ ] Step 4 says everything is already at that version, and no new push appears.
- [ ] In step 5, PC A gets an error that the part is checked out by blair. Nothing changes.
- [ ] Restoring a file that has unpushed edits, is open in SolidWorks, or was deleted is refused
  the same way.
- [ ] A restore doesn't change file names or locations (try restoring a push from before 6j: the
  files keep their new names).

## Part 7: Speed (on the team's real drive)

This part is read-only (only tick **Get** on folders), so it's safe to run on the real team drive.
Don't use **Check out all** or **Rename** here.

1. Point one PC at the real folder (Settings › CAD folder › Change), sign in, and time how long until the
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
| Installed app and SwRefs | `permafrost.exe` and `SwRefs.exe` side by side, usually in `%LOCALAPPDATA%\permafrost\` |
| Settings | `%APPDATA%\org.frc.cadsync\config.json` |
| Local index (safe to delete; it rebuilds) | `%APPDATA%\org.frc.cadsync\<drive id>\index.sqlite` |
| Files removed from the vault | `%APPDATA%\org.frc.cadsync\<drive id>\trash\` |
| Push records on Drive | `.permafrost/changesets/` in the shared drive (`.cadsync/changesets/` on drives from older versions) |
| Logs and crash reports | Settings › **Open logs folder** (`%LOCALAPPDATA%\org.frc.cadsync\logs`). Set `RUST_LOG=cadsync=debug` for more detail. |

## Results

Copy this table into your report. For every **fail**, include what you did, what happened, and
the log lines from around that time.

| Test | Pass / Fail | Notes |
| --- | --- | --- |
| 1. Automated tests | | |
| 2. SwRefs on a real assembly (incl. `replace`) | | |
| 3. Setup on both PCs (installer or source?) | | |
| 4a. Check-out blocks Drive website upload | | |
| 4b. Only one check-out | | |
| 4c. Keep forever | | |
| 5a. Assembly + parts arrive together, mates intact | | |
| 5b. Pre-push checks (8 cases) | | |
| 5c. Check out with parts, Check out all, Open | | |
| 6a. Read-only stops saves | | |
| 6b. Updates wait for SolidWorks (did `~$` appear?) | | |
| 6c. Crash during a push | | |
| 6d. Conflict copy kept | | |
| 6e. Take over a lock | | |
| 6f. Odd names | | |
| 6g. Deleted files restored, Delete for everyone | | |
| 6h. Only one copy runs | | |
| 6i. Expired sign-in | | |
| 6j. Rename a part, mates intact on PC B | | |
| 6j. Move a folder | | |
| 6j. Rename refused (teammate's check-out, open file, bad names) | | |
| 6j. Explorer rename (what happened?) | | |
| 6k. Restore a push, assembly + part both back, mates intact | | |
| 6k. Restore refused / already at that version | | |
| 7. Speed (times and file count) | | |
